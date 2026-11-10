//! First-run play choice, also available from the launcher's tools.
use super::*;
use crate::glass_ui as ui;
use horizon_launcher::dual::Source;

pub(super) fn initial_page(page: Page, seen: bool, explicit_route: bool) -> Page {
    if !seen && !explicit_route && page == Page::Dashboard {
        Page::StartGuide
    } else {
        page
    }
}

#[derive(Clone, Copy)]
enum Route {
    Microsoft,
    Local,
    Guest,
}

impl Launcher {
    fn choose_play_setup(&mut self, route: Route, cx: &mut Context<Self>) {
        self.settings.bodka.play_setup_seen = true;
        self.signup_from_guide = matches!(route, Route::Microsoft);
        if matches!(route, Route::Local) {
            self.show_page(Page::Dual);
            self.select_dual_source(Source::Tlauncher);
        } else {
            self.save_settings();
            self.show_page(Page::Dashboard);
            self.advance_guide_sign_in(cx);
        }
        cx.notify();
    }

    pub(super) fn advance_guide_sign_in(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.signup_from_guide {
            return false;
        }
        match self.owner {
            Some(true) => {
                self.signup_from_guide = false;
                self.owner_screen_open = false;
                self.open_direct_install(cx);
                true
            }
            Some(false) if !self.owner_screen_open => {
                self.sign_in_to_play();
                true
            }
            _ => false,
        }
    }

    pub(super) fn play_setup_page(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let rtl = i18n::is_rtl();
        let row = || {
            div()
                .flex()
                .flex_wrap()
                .gap(px(12.))
                .when(rtl, |d| d.flex_row_reverse())
        };
        let label = |s: &'static str, size, weight, alpha| {
            ui::text(size, weight, alpha)
                .w_full()
                .when(rtl, |d| d.text_right())
                .child(tr(s))
        };
        let option = |id: &'static str,
                      icon: &'static str,
                      title: &'static str,
                      description: &[&'static str],
                      action: &'static str,
                      kind: Kind,
                      route| {
            crate::glass::plate(14.)
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(215.))
                .max_w_full()
                .gap(px(12.))
                .p(px(20.))
                .child(
                    row()
                        .items_center()
                        .child(ui::icon(icon, 24., t.accent))
                        .child(ui::text(19., FontWeight::SEMIBOLD, 1.).child(tr(title))),
                )
                .child(div().flex().flex_col().flex_1().gap(px(4.)).children(description.iter().map(|s| label(s, 14., FontWeight::NORMAL, 0.90))))
                .child(button(id, action, kind, &t).on_click(
                    cx.listener(move |app, _: &ClickEvent, _, cx| app.choose_play_setup(route, cx)),
                ))
        };
        div().w_full().flex().flex_col().items_center().py(px(16.))
            .child(div().w_full().max_w(px(900.)).flex().flex_col().gap(px(15.))
                .child(label("Which Minecraft version do you use?",30.,FontWeight::BOLD,1.))
                .child(row()
                    .child(option("setup-official","user","I own Minecraft Java",
                        &["Sign in with the Microsoft account that owns the game.", "Choose a Java version and BODKA downloads it."],
                        "Continue with Microsoft",Kind::Primary,Route::Microsoft))
                    .child(option("setup-local","folder-01","Local or cracked copy",
                        &["A cracked copy has no verified game license.", "Find TLauncher and import local versions and mods.", "Playing Java in BODKA requires game ownership."],
                        "Check for TLauncher",Kind::Secondary,Route::Local)))
                .child(crate::glass::plate(12.).flex().flex_col().gap(px(6.)).p(px(18.))
                    .child(label("Java Edition minimum · desktop and laptop",17.,FontWeight::SEMIBOLD,1.))
                    .child(label("Windows 10 or 11 x64 · Four physical cores, Core i3-10100 or Ryzen 3 3100 class.",14.,FontWeight::NORMAL,0.9))
                    .child(label("Memory: 8 GB with a discrete GPU or 12 GB with integrated graphics.",14.,FontWeight::NORMAL,0.9))
                    .child(label("Graphics: Vulkan 1.3 and 2 GB of video memory. Storage: 2 GB minimum, with extra space for worlds and mods.",14.,FontWeight::NORMAL,0.9))
                    .child(row().child(button("setup-specs","Minecraft system requirements",Kind::Ghost,&t)
                        .on_click(|_,_,cx|cx.open_url("https://www.minecraft.net/en-us/store/minecraft-java-bedrock-edition-pc")))))
                .child(row().child(button("setup-guest","Browse as a guest",Kind::Ghost,&t)
                    .on_click(cx.listener(|app,_:&ClickEvent,_,cx|app.choose_play_setup(Route::Guest,cx))))))
    }
}

#[cfg(test)]
#[test]
fn guide_only_replaces_the_first_normal_home() {
    assert!(initial_page(Page::Dashboard, false, false) == Page::StartGuide);
    assert!(initial_page(Page::Dashboard, true, false) == Page::Dashboard);
    assert!(initial_page(Page::Dashboard, false, true) == Page::Dashboard);
    assert!(initial_page(Page::Settings, false, false) == Page::Settings);
}

#[cfg(all(windows, test, feature = "visual-tests"))]
#[test]
#[ignore = "real Windows rendering, local launcher detection and a cancellable Microsoft device request"]
fn two_onboarding_routes_use_the_real_scan_and_device_flow() {
    use gpui::AnyWindowHandle;
    use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
    let scratch = std::path::PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("isolated profile"));
    assert!(backend::data_dir().starts_with(&scratch) && backend::game_home().starts_with(&scratch));
    assert!(Accounts::load().accounts.is_empty());
    let passed=Arc::new(AtomicBool::new(false));
    let done=passed.clone();
    gpui_platform::application().with_assets(blocks::Assets).run(move |cx| {
        let window=cx.open_window(WindowOptions{
            window_bounds:Some(WindowBounds::Windowed(Bounds::centered(None,size(px(1150.),px(760.)),cx))),
            show:true,focus:false,..Default::default()
        },|w,cx|cx.new(|cx| {
            let mut app=Launcher::new(w,cx);
            app.collection.consent.answer(Default::default());
            app.opening=None;
            assert!(app.page==Page::StartGuide);
            app
        })).unwrap();
        cx.spawn(async move |cx| {
            let mut stage=0;
            for n in 0..260 {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let ready=window.update(cx,|app,_,_| match stage{
                    0=>n>3 && app.owner==Some(false),
                    1=>app.page==Page::Dual && !app.dual.scanning() && app.settings.bodka.dual_source==Source::Tlauncher,
                    _=>app.asking_owner() && matches!(app.sign_in_state,Some(SignInState::Code(_))),
                }).unwrap();
                if !ready {continue;}
                if stage<2 {
                    let any:AnyWindowHandle=window.into();
                    let image=cx.update(|cx|any.update(cx,|_,w,cx|{
                        w.refresh();w.draw(cx).clear(cx);w.render_to_image().unwrap()
                    }).unwrap());
                    image.save(scratch.join(["start-guide.png","local-check.png"][stage])).unwrap();
                }
                window.update(cx,|app,_,cx|match stage{
                    0=>app.choose_play_setup(Route::Local,cx),
                    1=>{app.show_page(Page::StartGuide);app.choose_play_setup(Route::Microsoft,cx);}
                    _=>{app.cancel_sign_in();assert!(Settings::load().bodka.play_setup_seen);done.store(true,Ordering::Relaxed);}
                }).unwrap();
                stage+=1;
                if stage==3 {break;}
            }
            cx.update(|cx|cx.quit());
        }).detach();
    });
    assert!(passed.load(Ordering::Relaxed),"local scan and Microsoft device prompt must both complete");
}
