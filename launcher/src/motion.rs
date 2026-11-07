//! The launcher's motion, as the website's launcher plays it (horizon-site's `launcher.css` and
//! Tailwind's defaults): hovers ease over the design's quick curve (150 ms,
//! `cubic-bezier(0.4, 0, 0.2, 1)`); toasts and a new page arrive as `hzl-pop` does (a fade and a
//! 6 px rise over 200 ms); a switch's knob slides for 200 ms. Under reduced motion
//! ([`crate::sky::motion`]) nothing moves: everything is drawn where it ends. Menus open at once:
//! an animation inside a `deferred` menu kept its first frame (invisible) under GPUI Fast.
use gpui::{px, Animation, AnimationExt, AnyElement, Div, ElementId, IntoElement, Styled};
use horizon_launcher::bodka_settings::Motion;
use horizon_theme as tokens;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use std::time::Instant;

thread_local! {
    static MODE: Cell<Motion> = const { Cell::new(Motion::Balanced) };
    static ACTIVE: Cell<bool> = const { Cell::new(true) };
    static REDUCED: RefCell<Option<(Instant, bool)>> = const { RefCell::new(None) };
}

pub fn configure(mode: Motion, active: bool) {
    MODE.set(mode);
    ACTIVE.set(active);
}

/// Cache Windows' accessibility preference briefly; changes apply without restarting.
pub fn system_reduced() -> bool {
    REDUCED.with_borrow_mut(|cached| {
        if let Some((at, reduced)) = cached {
            if at.elapsed() < Duration::from_secs(2) {
                return *reduced;
            }
        }
        let mut reduced = matches!(
            std::env::var("HORIZON_LAUNCHER_MOTION").as_deref(),
            Ok("off" | "reduced")
        );
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION,
            };
            let mut enabled: i32 = 1;
            // SAFETY: this action writes one BOOL to the valid pointer.
            let ok = unsafe {
                SystemParametersInfoW(
                    SPI_GETCLIENTAREAANIMATION,
                    0,
                    (&mut enabled as *mut i32).cast(),
                    0,
                )
            };
            reduced |= ok != 0 && enabled == 0;
        }
        #[cfg(target_os = "macos")]
        {
            reduced |= objc2_app_kit::NSWorkspace::sharedWorkspace()
                .accessibilityDisplayShouldReduceMotion();
        }
        *cached = Some((Instant::now(), reduced));
        reduced
    })
}

pub fn continuous() -> bool {
    MODE.get().moves(system_reduced(), ACTIVE.get())
}
pub fn spatial() -> bool {
    !still() && MODE.get() != Motion::Reduced && !system_reduced()
}
pub fn duration(ms: u64) -> Duration {
    Duration::from_millis(MODE.get().duration_ms(ms, system_reduced()).max(1))
}
pub fn fade(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
pub fn stagger(elapsed: f32, delay: f32, length: f32) -> f32 {
    if still() {
        1.
    } else {
        fade((elapsed - delay) / length.max(0.001))
    }
}
/// Bounded critically damped spring, settled even after suspension.
pub fn spring(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t >= 1. {
        1.
    } else {
        (1. - (1. + 10. * t) * (-10. * t).exp()).clamp(0., 1.)
    }
}

/// `hzl-pop`: `0.2s cubic-bezier(0.2, 0.7, 0.2, 1)`, from transparent and 6 px lower.
pub const POP: tokens::Motion = tokens::Motion {
    name: "pop",
    ms: 200,
    curve: [0.2, 0.7, 0.2, 1.],
};
pub const POP_RISE: f32 = 6.;

/// A switch's knob (`transition-[translate] duration-200`, Tailwind's default curve).
const SLIDE: tokens::Motion = tokens::Motion {
    name: "slide",
    ms: 200,
    curve: tokens::QUICK.curve,
};

/// Whether the player's system asks for reduced motion (or `HORIZON_LAUNCHER_MOTION=off`).
pub fn still() -> bool {
    MODE.get() == Motion::Off || !ACTIVE.get()
}

/// `motion` as GPUI plays it: its length on its own curve; at its end from the first frame under
/// reduced motion.
pub fn eased(motion: tokens::Motion) -> Animation {
    let animation = Animation::new(duration(motion.ms));
    if still() {
        return animation.with_easing(|_| 1.);
    }
    animation.with_easing(move |t| motion.ease(t))
}

thread_local! {
    /// Each switch's last state, and when it last changed.
    static SWITCHES: RefCell<HashMap<String, (bool, Option<Instant>)>> = RefCell::new(HashMap::new());
}

/// When switch `id`'s knob started sliding towards `on`, while it still slides: a switch seen
/// for the first time, or changed long ago, is already there.
fn slide(id: &str, on: bool, now: Instant) -> Option<Instant> {
    SWITCHES.with_borrow_mut(|switches| {
        let seen = switches.entry(id.to_string()).or_insert((on, None));
        if seen.0 != on {
            *seen = (on, Some(now));
        }
        seen.1
            .filter(|since| now.duration_since(*since) < SLIDE.duration())
    })
}

/// A switch's knob at `left` px when off and `left + travel` when on, sliding there when the
/// switch `id` changed in the last 200 ms.
pub fn knob(id: &str, on: bool, knob: Div, left: f32, travel: f32) -> AnyElement {
    let at = move |p: f32| left + travel * if on { p } else { 1. - p };
    if !spatial() {
        return knob.left(px(at(1.))).into_any_element();
    }
    match slide(id, on, Instant::now()) {
        Some(since) => {
            // Each slide gets its own animation id, from when it started, so it plays once.
            static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let start = *START.get_or_init(Instant::now);
            let key = since.saturating_duration_since(start).as_micros() as usize;
            knob.with_animation(
                ElementId::named_usize(format!("{id}-knob"), key),
                eased(SLIDE),
                move |d, p| d.left(px(at(p))),
            )
            .into_any_element()
        }
        None => knob.left(px(at(1.))).into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_switch_slides_only_after_it_changed() {
        let t0 = Instant::now();
        assert_eq!(slide("test-a", true, t0), None, "first seen: already there");
        assert_eq!(slide("test-a", true, t0 + Duration::from_millis(50)), None);
        let flip = t0 + Duration::from_millis(100);
        assert_eq!(slide("test-a", false, flip), Some(flip));
        assert_eq!(
            slide("test-a", false, flip + Duration::from_millis(150)),
            Some(flip),
            "the same slide while it runs"
        );
        assert_eq!(
            slide("test-a", false, flip + Duration::from_millis(250)),
            None
        );
    }
}
