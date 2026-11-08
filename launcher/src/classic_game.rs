//! Mojang's public Minecraft Classic client in a dedicated window owned by BODKA.
use super::*;

const SITE: &str = "https://classic.minecraft.net/";

pub(super) fn button(id: &'static str, t: &Theme, cx: &mut Context<Launcher>) -> Stateful<Div> {
    crate::glass_ui::quiet(id, tr("Play Minecraft Classic as a guest"), 32., t).on_click(
        cx.listener(|this, _: &ClickEvent, _, cx| {
            #[cfg(windows)]
            this.open_classic(cx);
            #[cfg(not(windows))]
            {
                let _ = this;
                cx.open_url(SITE);
            }
        }),
    )
}

#[cfg(windows)]
pub(super) struct GuestWindow {
    view: Result<wry::WebView, String>,
}

#[cfg(windows)]
impl Render for GuestWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(gpui::rgb(0x183023))
            .text_color(gpui::white())
            .p(px(20.))
            .children(self.view.as_ref().err().cloned())
    }
}

#[cfg(windows)]
impl GuestWindow {
    fn new(win: &mut Window) -> Self {
        win.set_window_title(tr("Minecraft Classic · BODKA"));
        let mut context =
            wry::WebContext::new(Some(backend::data_dir().join("minecraft-classic-web")));
        let view = wry::WebViewBuilder::new_with_web_context(&mut context)
            .with_url(SITE)
            .with_navigation_handler(|url| url.starts_with(SITE))
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_devtools(false)
            .build(win)
            .map_err(|e| {
                trf(
                    "Could not open Minecraft Classic. Check WebView2 and your connection. {error}",
                    &[("error", &e.to_string())],
                )
            });
        Self { view }
    }
}

#[cfg(windows)]
impl Launcher {
    fn open_classic(&mut self, cx: &mut Context<Self>) {
        if self
            .library
            .classic
            .is_some_and(|h| h.update(cx, |_, w, _| w.activate_window()).is_ok())
        {
            return;
        }
        let bounds = Bounds::centered(None, size(px(960.), px(600.)), cx);
        match cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |w, cx| cx.new(|_| GuestWindow::new(w)),
        ) {
            Ok(handle) => self.library.classic = Some(handle),
            Err(e) => crate::glass_toast::say(trf(
                "Could not open Minecraft Classic. Check WebView2 and your connection. {error}",
                &[("error", &e.to_string())],
            )),
        }
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    #[ignore = "opens the public Minecraft Classic site in an isolated native WebView2 window"]
    fn official_classic_renders_game_canvas_without_an_account() {
        let scratch =
            std::path::PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("scratch"));
        assert!(backend::data_dir().starts_with(scratch));
        assert!(Accounts::load().accounts.is_empty());
        let result = Arc::new(Mutex::new(false));
        let observed = result.clone();
        gpui_platform::application().run(move |cx| {
            let handle = cx.open_window(WindowOptions { show:false, focus:false, ..Default::default() },
                |w,cx| cx.new(|_| GuestWindow::new(w))).expect("open guest window");
            cx.spawn(async move |cx| {
                for _ in 0..65 {
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                    let callback = observed.clone();
                    handle.update(cx, |view, _, _| {
                        let web = view.view.as_ref().expect("WebView2 initialized");
                        web.evaluate_script_with_callback(
                            "location.origin === 'https://classic.minecraft.net' && !!document.querySelector('canvas') && !!document.getElementById('hotbar') && document.readyState === 'complete'",
                            move |v| { if v == "true" { *callback.lock().unwrap() = true; } }
                        ).unwrap();
                    }).unwrap();
                    if *observed.lock().unwrap() { break; }
                }
                cx.update(|cx| cx.quit());
            }).detach();
        });
        assert!(
            *result.lock().unwrap(),
            "the official game drew a canvas with a hotbar in WebView2"
        );
    }
}
