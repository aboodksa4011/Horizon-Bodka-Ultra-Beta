//! Opt-in real GPUI layout and wheel regression. The test window is hidden.
use super::*;
use gpui::{point, AnyWindowHandle, PlatformInput, ScrollDelta, ScrollEvent};
use std::sync::Arc;

#[test]
#[cfg(windows)]
#[ignore = "requires an isolated BODKA_TEST_SCRATCH profile and Windows graphics"]
fn wheel_scrolls_the_settings_pane_at_minimum_window_size() {
    let scratch = std::path::PathBuf::from(
        std::env::var_os("BODKA_TEST_SCRATCH").expect("isolated test profile"),
    );
    assert!(
        backend::data_dir().starts_with(&scratch) && backend::game_home().starts_with(&scratch)
    );
    let observed = Arc::new(Mutex::new(None));
    let result = observed.clone();
    gpui_platform::application()
        .with_assets(blocks::Assets)
        .run(move |cx| {
            let bounds = Bounds::centered(None, size(px(960.), px(600.)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        show: false,
                        focus: false,
                        ..Default::default()
                    },
                    |window, cx| {
                        cx.new(|cx| {
                            let mut launcher = Launcher::new(window, cx);
                            launcher.opening = None;
                            launcher.collection.consent.answer(Default::default());
                            launcher.page = Page::Settings;
                            launcher
                        })
                    },
                )
                .unwrap();
            let any: AnyWindowHandle = handle.into();
            any.update(cx, |_, window, _| window.resize(size(px(960.), px(600.))))
                .unwrap();
            cx.spawn(async move |cx| {
                // Win32 schedules SetWindowPos. Give its WM_SIZE callback a turn on the GUI thread.
                for _ in 0..40 {
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                    if any
                        .update(cx, |_, window, _| window.viewport_size().height > px(100.))
                        .unwrap()
                    {
                        break;
                    }
                }
                any.update(cx, |view, window, cx| {
                    let entity = view.downcast::<Launcher>().ok().unwrap();
                    window.draw(cx).clear(cx);
                    let pane = entity.read(cx).settings_pane.handle.clone();
                    let outer = entity.read(cx).content_scroll.handle.clone();
                    let bounds = pane.bounds();
                    eprintln!(
                        "SCROLL_LAYOUT window={:?} pane={bounds:?} first={:?} consent={}",
                        window.viewport_size(),
                        pane.bounds_for_item(0),
                        entity.read(cx).asking_consent()
                    );
                    let at = bounds.center();
                    let before = pane.offset().y;
                    window.dispatch_event(
                        PlatformInput::MouseMove(gpui::MouseMoveEvent {
                            position: at,
                            pressed_button: None,
                            modifiers: Default::default(),
                        }),
                        cx,
                    );
                    window.dispatch_event(
                        PlatformInput::Scroll(ScrollEvent {
                            position: at,
                            delta: ScrollDelta::Pixels(point(px(0.), px(-220.))),
                            ..Default::default()
                        }),
                        cx,
                    );
                    window.draw(cx).clear(cx);
                    let after = pane.offset().y;
                    pane.scroll_to_top_of_item(8);
                    window.refresh();
                    window.draw(cx).clear(cx);
                    *result.lock().unwrap() = Some((
                        f32::from(bounds.size.height),
                        f32::from(pane.max_offset().y),
                        f32::from(before - after),
                        f32::from(outer.offset().y),
                        f32::from(-pane.offset().y),
                    ));
                })
                .unwrap();
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    let (height, max, wheel, outer, last) = observed.lock().unwrap().expect("rendered settings");
    eprintln!(
        "SETTINGS_SCROLL viewport={height} max={max} wheel={wheel} outer={outer} last={last}"
    );
    assert!(height > 0. && height < 600. && max > 0. && wheel > 0. && outer == 0. && last > wheel);
}
