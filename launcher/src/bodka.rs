//! BODKA 11 shell. All status values come from the existing launcher state.
use super::*;
use crate::glass_ui as ui;
use gpui::{linear_color_stop, linear_gradient, point, rgb, ObjectFit};
use horizon_launcher::bodka_settings::{Motion, Theme as BodkaTheme};

pub const BOOT_TEXT: &str = "الدفاع عن العقيدة الإسلامية والمجتمع والوطن واجب على كل مواطن، ويبيّن النظام أحكام الخدمة العسكرية";

fn text(s: impl Into<SharedString>, size: f32) -> Div {
    ui::text(size, FontWeight::NORMAL, 1.).child(s.into())
}
fn card() -> Div {
    crate::glass::plate(8.)
        .flex()
        .flex_col()
        .gap(px(12.))
        .p(px(22.))
}
fn row() -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(12.))
        .when(i18n::is_rtl(), |d| d.flex_row_reverse())
}

impl Launcher {
    pub(super) fn bodka_navigation(&self, cx: &mut Context<Self>) -> Div {
        let pages = [
            (Page::Dashboard, "My libraries", "archive"),
            (Page::Play, "Play game", "play"),
            (Page::Versions, "Versions", "archive"),
            (Page::Dual, "BODKA Dual Launcher", "package"),
            (Page::Downloads, "Downloads", "download-04"),
            (Page::Worlds, "Worlds", "globe-02"),
            (Page::Mods, "Mods", "package"),
            (Page::Performance, "Performance", "computer"),
            (Page::Settings, "Settings", "sliders-horizontal"),
            (Page::Diagnostics, "Diagnostics", "search-01"),
            (Page::Servers, "Servers", "server"),
            (Page::Notes, "Patch notes", "notes"),
            (Page::About, "About", "information-circle"),
        ];
        let pages:Vec<_>=pages.into_iter().filter(|(p,_,_)|self.library.sidebar_tools || matches!(p,Page::Dashboard|Page::Downloads|Page::Settings) || *p==self.page).collect();
        let current = pages.iter().position(|(page, ..)| *page == self.page);
        let previous = pages
            .iter()
            .position(|(page, ..)| Some(*page) == self.previous_page)
            .or(current);
        let mut nav = div().relative().flex().flex_col().gap(px(3.)).px(px(12.));
        if let Some(index) = current {
            let from = previous.unwrap_or(index) as f32 * 42. + 9.;
            let to = index as f32 * 42. + 9.;
            let indicator = div()
                .absolute()
                .right(px(12.))
                .w(px(3.))
                .h(px(21.))
                .bg(self.theme.accent);
            nav = if motion::spatial() {
                nav.child(indicator.with_animation(
                    ElementId::named_usize("bodka-nav-indicator", self.switches),
                    motion::eased(motion::POP),
                    move |d, p| d.top(px(from + (to - from) * motion::spring(p))),
                ))
            } else {
                nav.child(indicator.top(px(to)))
            };
        }
        for (i, (page, label, icon)) in pages.into_iter().enumerate() {
            let selected = self.page == page;
            let t = self.theme;
            nav = nav.child(
                keyboard(
                    div()
                        .id(ElementId::named_usize("bodka-nav", i))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .h(px(39.))
                        .px(px(14.))
                        .when(i18n::is_rtl(), |d| d.flex_row_reverse())
                        .rounded(px(4.))
                        .cursor_pointer()
                        .bg(if selected {
                            t.accent.opacity(0.15)
                        } else {
                            gpui::transparent_black()
                        })
                        .hover(|s| s.bg(t.accent.opacity(0.12)))
                        .active(|s| s.bg(t.accent.opacity(0.24)))
                        .child(ui::icon(
                            icon,
                            18.,
                            if selected { t.accent } else { t.muted },
                        ))
                        .child(text(tr(label), 14.).font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })),
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.go(page);
                    cx.notify();
                })),
            );
        }
        nav.child(ui::quiet_icon("library-more-tools",Some("sliders-horizontal"),tr(if self.library.sidebar_tools {"Fewer sections"}else{"Launcher tools"}),34.,&self.theme).on_click(cx.listener(|this,_:&ClickEvent,_,cx|{this.library.sidebar_tools = !this.library.sidebar_tools;cx.notify();})))
    }

    pub(super) fn bodka_sidebar(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        crate::glass::plate(8.)
            .flex()
            .flex_col()
            .flex_none()
            .w(px(240.))
            .my(px(14.))
            .when(i18n::is_rtl(), |d| d.mr(px(14.)))
            .when(!i18n::is_rtl(), |d| d.ml(px(14.)))
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .py(px(20.))
                    .child(
                        text("Bodka", 42.)
                            .font_family("Segoe Script")
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.accent),
                    )
                    .child(
                        text("Ultra", 17.)
                            .font_family("Segoe Script")
                            .text_color(t.muted),
                    )
                    .child(div().w(px(48.)).h(px(1.)).bg(t.accent.opacity(0.6))),
            )
            .child(
                div()
                    .id("bodka-side-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.bodka_navigation(cx)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .p(px(16.))
                    .border_t_1()
                    .border_color(t.accent.opacity(0.15))
                    .when(self.owner != Some(true), |d| d.child(crate::classic_game::button("sidebar-classic", &t, cx)))
                    .child(
                        text(
                            if self.owner == Some(true) {
                                tr("Account ready")
                            } else {
                                tr("BODKA Guest Mode")
                            },
                            12.,
                        )
                        .text_color(t.accent),
                    )
                    .child(
                        text(
                            trf("Launcher {version}", &[("version", backend::VERSION)]),
                            11.,
                        )
                        .id("bodka-version")
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.version_clicked();
                                cx.notify();
                            },
                        )),
                    ),
            )
    }

    pub(super) fn bodka_worlds(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .child(ui::page_head(
                tr("Worlds"),
                tr("Local worlds for the selected instance. Back up before making changes."),
            ))
            .children(self.glass_worlds(cx))
    }

    pub(super) fn bodka_downloads(&mut self, cx: &mut Context<Self>) -> Div {
        use horizon_launcher::downloads::State;
        let t = self.theme;
        let mut page = div().flex().flex_col().gap(px(16.)).child(ui::page_head(
            tr("Downloads"),
            tr("Live transfers, preparation and verification."),
        ));
        if self.transfers.is_empty() {
            page = page.child(card().child(text(tr("No active downloads"), 18.)).child(
                button("downloads-versions", "Browse versions", Kind::Primary, &t).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.go(Page::Versions);
                        cx.notify();
                    }),
                ),
            ));
        }
        for (i, transfer) in self.transfers.clone().into_iter().enumerate() {
            let active = self
                .installing
                .as_ref()
                .is_some_and(|(tag, _)| *tag == transfer.tag);
            let fraction = transfer.fraction();
            let total = if transfer.total > 0 {
                megabytes(transfer.total)
            } else {
                tr("Unknown size").to_string()
            };
            let mut details = card()
                .child(
                    row()
                        .justify_between()
                        .child(text(transfer.tag.clone(), 18.))
                        .child(text(tr(transfer.state.label()), 14.).text_color(t.accent)),
                )
                .child(text(transfer.file.clone(), 13.).font_family(theme::MONO))
                .child(progress_bar(fraction.map(|p| (p, p)), &t))
                .child(
                    row()
                        .justify_between()
                        .child(text(
                            trf(
                                "{done} of {total}",
                                &[("done", &megabytes(transfer.downloaded)), ("total", &total)],
                            ),
                            13.,
                        ))
                        .child(text(
                            fraction
                                .map(|p| format!("{:.0}%", p * 100.))
                                .unwrap_or_else(|| tr("Unknown size").to_string()),
                            13.,
                        )),
                );
            if transfer.state == State::Downloading {
                details = details.child(
                    row()
                        .child(text(
                            trf(
                                "Speed: {speed} MB/s",
                                &[(
                                    "speed",
                                    &format!("{:.2}", transfer.bytes_per_second / 1_048_576.),
                                )],
                            ),
                            13.,
                        ))
                        .children(transfer.seconds_left().map(|seconds| {
                            text(
                                trf(
                                    "About {seconds} seconds remaining",
                                    &[("seconds", &seconds.to_string())],
                                ),
                                13.,
                            )
                        })),
                );
            }
            let mut actions = row();
            if active && transfer.state.controllable() {
                if let Some(control) = &self.download_control {
                    let toggle = control.clone();
                    let cancel = control.clone();
                    let paused = transfer.state == State::Paused;
                    actions = actions
                        .child(
                            button(
                                format!("transfer-toggle-{i}"),
                                if paused { "Resume" } else { "Pause" },
                                Kind::Secondary,
                                &t,
                            )
                            .on_click(move |_, _, _| {
                                if paused {
                                    toggle.resume();
                                } else {
                                    toggle.pause();
                                }
                            }),
                        )
                        .child(
                            button(format!("transfer-cancel-{i}"), "Cancel", Kind::Ghost, &t)
                                .on_click(move |_, _, _| cancel.cancel()),
                        );
                }
            } else if matches!(
                transfer.state,
                State::Interrupted | State::Failed | State::Cancelled
            ) && self.installing.is_none()
            {
                let release = match &self.releases {
                    Load::Ready(releases) => {
                        releases.iter().find(|r| r.tag == transfer.tag).cloned()
                    }
                    _ => None,
                };
                if let Some(release) = release {
                    actions = actions.child(
                        button(
                            format!("transfer-retry-{i}"),
                            if transfer.state == State::Interrupted {
                                "Resume"
                            } else {
                                "Try again"
                            },
                            Kind::Primary,
                            &t,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.install(&release);
                                cx.notify();
                            },
                        )),
                    );
                } else {
                    actions = actions.child(text(
                        tr("Reload the version catalog to resume this transfer."),
                        13.,
                    ));
                }
            }
            let tag = transfer.tag.clone();
            let expanded = self.download_details.as_deref() == Some(tag.as_str());
            actions = actions.child(
                button(
                    format!("transfer-details-{i}"),
                    if expanded {
                        "Hide details"
                    } else {
                        "Show details"
                    },
                    Kind::Ghost,
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.download_details = if expanded { None } else { Some(tag.clone()) };
                    cx.notify();
                })),
            );
            if expanded {
                details=details.child(text(tr("Source: official Horizon releases. Installation requires a valid signed SHA-256 checksum."),13.))
                    .child(text(horizon_launcher::downloads::directory().display().to_string(),12.).font_family(theme::MONO));
            }
            page = page.child(
                details
                    .child(actions)
                    .children(transfer.error.map(|e| notice(e, &t))),
            );
        }
        if self.preparing {
            page = page.child(
                card()
                    .child(text(tr("Preparing game files"), 18.))
                    .child(text(
                    tr("Preparation is running. Detailed output is available on the game page."),
                    13.,
                )),
            );
        }
        page.child(
            button("downloads-open", "Open folder", Kind::Secondary, &t).on_click(|_, _, cx| {
                let path = horizon_launcher::downloads::directory();
                let _ = std::fs::create_dir_all(&path);
                crate::glass_toast::open(cx, &path);
            }),
        )
    }

    pub(super) fn bodka_performance(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let samples = self.perf.ui_frame_times();
        let mean = if samples.is_empty() {
            tr("Waiting for measurements").to_string()
        } else {
            format!(
                "{:.2} ms",
                samples.iter().sum::<f32>() / samples.len() as f32
            )
        };
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(ui::page_head(
                tr("Performance"),
                tr("Measurements from this launcher session. Game FPS is measured separately."),
            ))
            .child(
                row()
                    .child(
                        card()
                            .flex_1()
                            .child(text(tr("UI frame interval"), 14.))
                            .child(text(mean, 28.)),
                    )
                    .child(
                        card()
                            .flex_1()
                            .child(text(tr("Recorded frames"), 14.))
                            .child(text(samples.len().to_string(), 28.)),
                    ),
            )
            .child(
                card()
                    .child(self.performance_row(cx))
                    .child(self.bodka_motion_settings(cx)),
            )
            .child(
                button(
                    "bodka-bench-folder",
                    "Open measurement reports",
                    Kind::Secondary,
                    &t,
                )
                .on_click(|_, _, cx| {
                    let path = backend::data_dir().join("benchmarks");
                    let _ = std::fs::create_dir_all(&path);
                    crate::glass_toast::open(cx, &path);
                }),
            )
    }

    pub(super) fn bodka_diagnostics(&self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let configured = account::sign_in_available();
        let prepared = self.selected().is_some_and(|tag| backend::is_prepared(tag));
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(ui::page_head(
                tr("Diagnostics"),
                tr("Local checks and logs. No account secrets are displayed."),
            ))
            .child(card().child(text(tr("Microsoft sign-in"), 16.)).child(text(
                if configured {
                    tr("Configured")
                } else {
                    tr("This build needs a Microsoft application ID for sign-in.")
                },
                14.,
            )))
            .child(card().child(text(tr("Game resources"), 16.)).child(text(
                if prepared {
                    tr("Ready")
                } else {
                    tr("Needs preparing")
                },
                14.,
            )))
            .child(card().child(text(tr("Launcher data folder"), 16.)).child(
                text(backend::data_dir().display().to_string(), 13.).font_family(theme::MONO),
            ))
            .child(
                row()
                    .child(
                        button("bodka-logs", "Open logs", Kind::Primary, &t).on_click(
                            |_, _, cx| {
                                let path = backend::logs_dir();
                                let _ = std::fs::create_dir_all(&path);
                                crate::glass_toast::open(cx, &path);
                            },
                        ),
                    )
                    .child(
                        button(
                            "bodka-check-updates",
                            "Check for updates",
                            Kind::Secondary,
                            &t,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.check_updates();
                                cx.notify();
                            },
                        )),
                    ),
            )
            .children(self.update_error.clone().map(|e| notice(e, &t)))
    }

    pub(super) fn bodka_about(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(ui::page_head(
                tr("About"),
                horizon_launcher::branding::APP_NAME,
            ))
            .child(
                card()
                    .child(text("BODKA / 11", 34.).text_color(self.theme.accent))
                    .child(text(
                        tr("Built on Horizon. Your worlds and existing tools stay with you."),
                        16.,
                    ))
                    .child(text(
                        trf("Launcher {version}", &[("version", backend::VERSION)]),
                        13.,
                    ))
                    .child(
                        button(
                            "bodka-about-settings",
                            "Settings",
                            Kind::Secondary,
                            &self.theme,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.go(Page::Settings);
                                cx.notify();
                            },
                        )),
                    ),
            )
    }

    pub(super) fn bodka_motion_settings(&self, cx: &mut Context<Self>) -> Div {
        let options: Vec<_> = Motion::ALL.into_iter().map(|m| (m, m.label())).collect();
        let weak = cx.entity().downgrade();
        let picker = ui::segments(
            "bodka-motion",
            &options,
            self.settings.bodka.motion,
            &self.theme,
            move |mode, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.settings.bodka.motion = mode;
                    this.save_settings();
                    cx.notify();
                });
            },
        );
        ui::setting_row_stacked(
            tr("Motion level"),
            Some(
                text(tr("Windows reduced-motion preferences take priority."), 13.)
                    .into_any_element(),
            ),
            picker.into_any_element(),
        )
    }

    fn bodka_number(
        &self,
        key: &'static str,
        label: &'static str,
        get: fn(&horizon_launcher::bodka_settings::Settings) -> u8,
        set: fn(&mut horizon_launcher::bodka_settings::Settings, u8),
        max: u8,
        cx: &mut Context<Self>,
    ) -> Div {
        let value = get(&self.settings.bodka).min(max);
        let controls = row()
            .child(
                button(
                    SharedString::from(format!("{key}-less")),
                    "−",
                    Kind::Secondary,
                    &self.theme,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    set(&mut this.settings.bodka, value.saturating_sub(5));
                    this.save_settings();
                    cx.notify();
                })),
            )
            .child(text(value.to_string(), 14.).min_w(px(36.)).text_center())
            .child(
                button(
                    SharedString::from(format!("{key}-more")),
                    "+",
                    Kind::Secondary,
                    &self.theme,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    set(&mut this.settings.bodka, value.saturating_add(5).min(max));
                    this.save_settings();
                    cx.notify();
                })),
            );
        ui::setting_row(tr(label), None, Some(controls.into_any_element()))
    }

    pub(super) fn bodka_settings_card(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let mut themes = row();
        for (i, theme) in BodkaTheme::ALL.into_iter().enumerate() {
            let palette = theme.palette();
            themes = themes.child(
                keyboard(
                    div()
                        .id(ElementId::named_usize("bodka-theme", i))
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .p(px(12.))
                        .min_w(px(105.))
                        .flex_1()
                        .rounded(px(palette.radius))
                        .border_1()
                        .border_color(if self.settings.bodka.theme == theme {
                            t.accent
                        } else {
                            t.rule
                        })
                        .cursor_pointer()
                        .hover(|s| s.bg(t.accent.opacity(0.08)))
                        .child(
                            div()
                                .h(px(40.))
                                .w_full()
                                .rounded(px(palette.radius))
                                .bg(rgb(palette.background))
                                .border_1()
                                .border_color(rgb(palette.accent)),
                        )
                        .child(text(tr(theme.label()), 12.)),
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.settings.bodka.theme = theme;
                    this.settings.theme = None;
                    this.settings.appearance = if theme == BodkaTheme::WhiteMinimal {
                        backend::Appearance::Light
                    } else {
                        backend::Appearance::Dark
                    };
                    this.settings.accent =
                        horizon_launcher::themes::Accent::from_rgb(palette.accent);
                    this.save_settings();
                    this.theme = this.settings_theme(window);
                    cx.notify();
                })),
            );
        }
        let animated = self.settings.bodka.animated_background;
        let auto = self.settings.bodka.download_on_select;
        card().child(text(tr("BODKA personalization"),20.).text_color(t.accent)).child(themes)
            .child(self.bodka_motion_settings(cx))
            .child(ui::setting_row(tr("Animated background"),None,Some(switch("bodka-animated-background",animated,&t).on_click(cx.listener(|this, _: &ClickEvent, _,cx|{this.settings.bodka.animated_background= !this.settings.bodka.animated_background;this.save_settings();cx.notify();})).into_any_element())))
            .child(ui::setting_row(tr("Cherry sunset background"),None,Some(switch("bodka-sunset-background",self.settings.bodka.sunset_background,&t).on_click(cx.listener(|this, _: &ClickEvent,_,cx|{this.settings.bodka.sunset_background= !this.settings.bodka.sunset_background;this.save_settings();cx.notify();})).into_any_element())))
            .child(self.bodka_number("background-motion","Background motion intensity",|s|s.background_intensity,|s,v|s.background_intensity=v,100,cx))
            .child(self.home_background_row(cx))
            .child(self.bodka_number("background-dim","Background dimming",|s|s.dim,|s,v|s.dim=v,100,cx))
            .child(self.bodka_number("background-brightness","Background brightness",|s|s.brightness,|s,v|s.brightness=v,150,cx))
            .child(self.bodka_number("background-blur","Background blur",|s|s.background_blur,|s,v|s.background_blur=v,48,cx))
            .child(ui::setting_row(tr("Interface sounds"),Some(text(tr("Optional, quiet sounds. Disabled by default."),13.).into_any_element()),Some(switch("bodka-sound",self.settings.bodka.sound,&t).on_click(cx.listener(|this, _: &ClickEvent,_,cx|{this.settings.bodka.sound= !this.settings.bodka.sound;this.save_settings();cx.notify();})).into_any_element())))
            .child(self.bodka_number("sound-volume","Sound volume",|s|s.volume,|s,v|s.volume=v,100,cx))
            .child(button("bodka-replay-opening","Replay opening",Kind::Secondary,&t).on_click(cx.listener(|this, _: &ClickEvent,window,cx|{
                let focus=cx.focus_handle();window.focus(&focus,cx);
                this.opening=Some(Opening{started:None,reveal:None,focus});
                this.settings.bodka.opening_seen=false;this.settings.skip_opening=false;this.save_settings();cx.notify();
            })))
            .child(ui::setting_row(tr("Soft sound pack"),None,Some(switch("bodka-soft-sound",self.settings.bodka.soft_sound,&t).on_click(cx.listener(|this, _: &ClickEvent,_,cx|{this.settings.bodka.soft_sound= !this.settings.bodka.soft_sound;this.save_settings();cx.notify();})).into_any_element())))
            .child(ui::setting_row(tr("Download a version automatically when selected"),Some(text(tr("Only trusted public Horizon builds. Minecraft still requires its normal ownership checks."),13.).into_any_element()),Some(switch("bodka-download-select",auto,&t).on_click(cx.listener(|this, _: &ClickEvent, _,cx|{this.settings.bodka.download_on_select= !this.settings.bodka.download_on_select;this.save_settings();cx.notify();})).into_any_element())))
    }

    pub(super) fn bodka_background(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.settings.bodka.theme.palette();
        let settings = &self.settings.bodka;
        let animated = settings.animated_background && motion::continuous();
        let strength = f32::from(settings.background_intensity.min(100)) / 100.;
        let mut bg = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .bg(rgb(p.background));
        bg = bg.child(div().absolute().size_full().bg(linear_gradient(
            135.,
            linear_color_stop(rgb(p.accent).opacity(0.07), 0.),
            linear_color_stop(rgb(p.background), 1.),
        )));
        let light = div()
            .absolute()
            .top(px(-120.))
            .right(px(-100.))
            .w(px(550.))
            .h(px(600.))
            .bg(linear_gradient(
                120.,
                linear_color_stop(rgb(p.accent).opacity(0.08 * strength), 0.),
                linear_color_stop(rgb(p.background).opacity(0.), 1.),
            ));
        bg = if animated && self.home_background().is_none() && !settings.sunset_background {
            bg.child(light.with_animation(
                "bodka-background-breath",
                Animation::new(Duration::from_secs(14)).repeat(),
                move |d, p| d.opacity(0.35 + 0.65 * (p * std::f32::consts::PI).sin().abs()),
            ))
        } else {
            bg.child(light)
        };
        for i in 0..5 {
            bg = bg.child(
                div()
                    .absolute()
                    .right(px(60. + i as f32 * 80.))
                    .top_0()
                    .w(px(1.))
                    .h_full()
                    .bg(rgb(p.accent).opacity(0.035)),
            );
        }
        let blur = self.settings.bodka.background_blur.min(48);
        let picture = if let Some(path) = self.home_background() {
            let side = if blur > 0 { self.window_side().min(1280) } else { self.window_side() };
            Some((
                path,
                picture_cache::Look::Background(side, blur),
                picture_cache::Look::FrostedPhoto(blur),
            ))
        } else if self.settings.bodka.sunset_background {
            Some((
                std::path::PathBuf::from("bodka:cherry-sunset"),
                picture_cache::Look::BodkaFilm(blur, animated),
                picture_cache::Look::FrostedFilm(blur, animated),
            ))
        } else {
            None
        };
        let (sharp, frosted) = picture.map_or((None, None), |(path, base, frost)| {
            let sharp = self.picture(&path, base, cx);
            let blurred = if self.settings.performance() { None } else { self.picture(&path, frost, cx) };
            (sharp, blurred)
        });
        let frame = self.wallpaper_frame(sharp.as_deref(), cx);
        if let Some(image) = sharp {
            bg = bg.child(gpui::canvas(|_, _, _| {}, move |bounds, (), window, _| {
                let cover = ObjectFit::Cover.get_bounds(bounds, image.size(frame));
                let _ = window.paint_image(bounds, cover, gpui::Corners::default(), image.clone(), frame, false);
            }).absolute().size_full());
        }
        let brightness = f32::from(self.settings.bodka.brightness.min(150)) / 100.;
        let dim = f32::from(self.settings.bodka.dim.min(100)) / 100.;
        crate::glass::set_wallpaper(frosted, frame, brightness, dim);
        if brightness < 1. {
            bg = bg.child(
                div()
                    .absolute()
                    .size_full()
                    .bg(rgb(0).opacity(1. - brightness)),
            );
        } else if brightness > 1. {
            bg = bg.child(
                div()
                    .absolute()
                    .size_full()
                    .bg(rgb(0xffffff).opacity((brightness - 1.) * 0.5)),
            );
        }
        bg = bg.child(
            div()
                .absolute()
                .size_full()
                .bg(rgb(0).opacity(dim)),
        );
        bg.into_any_element()
    }

    pub(super) fn bodka_boot(
        &self,
        held: f32,
        reveal: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let opening = self.opening.as_ref()?;
        let fade = 1. - motion::fade(reveal);
        let progress = if motion::spatial() {
            motion::stagger(held, 0., 0.45)
        } else {
            1.
        };
        let quote = tr(BOOT_TEXT).to_string();
        // Shape complete Arabic lines once. Character-level styled runs split cursive clusters
        // and their paint slices assume LTR order. Reveal the shaped line with an RTL clip instead.
        let lines: Vec<String> = match quote.split_once("، ") {
            Some((first, second)) => vec![format!("{first}،"), second.to_owned()],
            None => vec![quote],
        };
        let quote_width = (self.window_width - 72.).clamp(320., 820.);
        let mut quote_block = div()
            .flex()
            .flex_col()
            .items_center()
            .w(px(quote_width))
            .min_h(px(86.));
        for (i, line) in lines.into_iter().enumerate() {
            let reveal = if motion::spatial() {
                motion::stagger(held, 0.6 + i as f32 * 1.7, 1.5)
            } else {
                1.
            };
            quote_block = quote_block.child(
                div().relative().w(px(quote_width)).h(px(40.)).child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .w(px(quote_width * reveal))
                        .h_full()
                        .overflow_hidden()
                        .child(
                            text(line, 21.)
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(quote_width))
                                .h_full()
                                .font_family("Segoe UI")
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_center()
                                .line_height(px(36.))
                                .whitespace_nowrap()
                                .text_color(rgb(if i == 0 { 0xc85f64 } else { 0x43b87e })),
                        ),
                ),
            );
        }
        let status = match &self.releases {
            Load::Loading => tr("Loading official version catalog…"),
            Load::Ready(_) => tr("Launcher ready"),
            Load::Failed(_) => tr("Catalog unavailable. Opening local launcher…"),
        };
        Some(
            div()
                .id("bodka-opening")
                .occlude()
                .track_focus(&opening.focus)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(15.))
                .px(px(36.))
                .bg(linear_gradient(
                    145.,
                    linear_color_stop(rgb(0x090909), 0.),
                    linear_color_stop(rgb(0x241a14), 1.),
                ))
                .opacity(fade)
                .child(
                    div()
                        .relative()
                        .opacity(progress)
                        .top(px(12. * (1. - motion::spring(progress))))
                        .child(
                            text("Bodka", 68. + 8. * progress)
                                .font_family("Segoe Script")
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0xa77b52))
                                .shadow(vec![gpui::BoxShadow {
                                    color: rgb(0x38281f).opacity(0.5).into(),
                                    offset: point(px(0.), px(8.)),
                                    blur_radius: px(20.),
                                    spread_radius: px(1.),
                                    inset: false,
                                }]),
                        ),
                )
                .child(
                    text("bodka", 18.)
                        .font_family("Segoe Script")
                        .text_center()
                        .text_color(rgb(0xf5f5f5))
                        .opacity(motion::stagger(held, 0.1, 0.3)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .w(px(250.))
                        .when(matches!(self.releases, Load::Loading), |d| {
                            d.child(progress_bar(None, &self.theme))
                        })
                        .child(text(status, 12.).text_center().text_color(rgb(0xbeb7b0))),
                )
                .child(quote_block)
                .child(
                    gpui::img("bodka/helicopter-clean.png")
                        .w(px(120.))
                        .h(px(120.))
                        .object_fit(ObjectFit::Contain)
                        .opacity(0.55 * motion::stagger(held, 1.6, 0.5)),
                )
                .child(
                    div()
                        .min_h(px(48.))
                        .children((held >= OPENING_HOLD.as_secs_f32()).then(|| {
                            keyboard(
                                div()
                                    .id("bodka-opening-ok")
                                    .min_w(px(150.))
                                    .px(px(30.))
                                    .py(px(12.))
                                    .rounded(px(6.))
                                    .bg(rgb(0))
                                    .border_1()
                                    .border_color(rgb(0xa77b52).opacity(0.5))
                                    .text_color(rgb(0xf5f5f5))
                                    .text_center()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x151515)))
                                    .child(tr("OK")),
                                &self.theme,
                            )
                            .on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.acknowledge_opening();
                                    cx.notify();
                                },
                            ))
                        })),
                )
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                    cx.stop_propagation();
                    if held >= OPENING_HOLD.as_secs_f32() && event.keystroke.key == "enter" {
                        this.acknowledge_opening();
                        cx.notify();
                    }
                }))
                .into_any_element(),
        )
    }
}
