//! BODKA's local Java installation catalog. Disk and network work stay off GPUI's thread.
use super::*;
use crate::glass_ui as ui;
use horizon_launcher::dual::{self, Entry, Origin, Source, Summary};
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Action {
    Import,
    Play,
    Repair,
}

#[derive(Default)]
pub(super) struct Catalog {
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    rows: Vec<Entry>,
    bindings: Vec<(Origin, String)>,
    summary: Option<Summary>,
    action: Option<Action>,
    notice: Option<String>,
    expanded: Option<Origin>,
    limit: usize,
}

impl Catalog {
    pub(super) fn scanning(&self) -> bool {self.cancel.is_some()}
}

impl Drop for Catalog {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl Launcher {
    pub(super) fn select_dual_source(&mut self, source: Source) {
        if self.settings.bodka.dual_source != source {
            self.settings.bodka.dual_folder = None;
        }
        self.settings.bodka.dual_source = source;
        self.save_settings();
        self.scan_dual(false);
    }
    pub(super) fn scan_dual(&mut self, all_drives: bool) {
        let state = &mut self.dual;
        if let Some(old) = state.cancel.take() {
            old.store(true, Ordering::Relaxed);
        }
        state.generation += 1;
        state.rows.clear();
        state.notice = None;
        state.summary = None;
        state.expanded = None;
        state.limit = 20;
        let generation = state.generation;
        let manual = self.settings.bodka.dual_folder.clone();
        let source = self.settings.bodka.dual_source;
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel = Some(cancel.clone());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::DualBindings(generation, dual::imported()));
            let mut summary = dual::discover_for(manual, source, all_drives, &cancel, |entries| {
                let _ = tx.send(Msg::DualFound(generation, entries));
            });
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            summary.installed = dual::InstalledLauncher::detect();
            let _ = tx.send(Msg::DualScanned(generation, summary));
        });
    }

    pub(super) fn dual_found(&mut self, g: u64, rows: Vec<Entry>) {
        if self.dual.generation == g {
            self.dual.rows.extend(rows);
        }
    }
    pub(super) fn dual_bindings(&mut self, g: u64, bindings: Vec<(Origin, String)>) {
        if self.dual.generation == g {
            self.dual.bindings = bindings;
        }
    }
    pub(super) fn dual_scanned(&mut self, g: u64, summary: Summary) {
        if self.dual.generation == g {
            self.dual.summary = Some(summary);
            self.dual.cancel = None;
        }
    }

    fn dual_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Choose game folder").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| {
                        this.set_dual_folder(Some(path));
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    fn set_dual_folder(&mut self, path: Option<PathBuf>) {
        self.settings.bodka.dual_folder = path;
        let all_drives = self.settings.bodka.dual_folder.is_none();
        self.save_settings();
        self.scan_dual(all_drives);
    }
    fn dual_action(&mut self, index: usize, action: Action) {
        if self.dual.action.is_some()
            || self.client.is_some()
            || self.starting_client.is_some()
            || self.preparing
        {
            return;
        }
        let Some(entry) = self.dual.rows.get(index).cloned() else {
            return;
        };
        self.dual.action = Some(action);
        self.dual.notice = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = dual::import(&entry).map(|instance| instance.id);
            let _ = tx.send(Msg::DualImported(entry.origin, result));
        });
    }
    pub(super) fn dual_imported(&mut self, origin: Origin, result: Result<String, String>) {
        let Some(action) = self.dual.action.take() else {
            return;
        };
        let instance = match result.and_then(|id| instances::find(&id)) {
            Ok(instance) => instance,
            Err(error) => {
                self.dual.notice = Some(error);
                return;
            }
        };
        self.dual.bindings.retain(|(key, _)| key != &origin);
        self.dual.bindings.push((origin, instance.id.clone()));
        self.reload_instances();
        match action {
            Action::Import => self.dual.notice = Some(tr("Instance imported").into()),
            Action::Play => {
                if self.client.is_some() || self.starting_client.is_some() || self.preparing {
                    self.dual.notice = Some(tr("Another game operation is active").into());
                    return;
                }
                self.request_java_action(&instance, libraries::JavaAction::Play);
            }
            Action::Repair => {
                if self.owner == Some(false) {
                    self.owner_screen_open = true;
                    self.dual.notice = Some(account::NoOwner::in_window());
                    return;
                }
                self.dual.action = Some(Action::Repair);
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let result = backend::repair_java(&instance, &|line| {
                        let _ = tx.send(Msg::DualRepairLine(line));
                    });
                    let _ = tx.send(Msg::DualRepaired(result));
                });
            }
        }
    }
    pub(super) fn dual_repair_line(&mut self, line: String) {
        self.dual.notice = Some(line);
    }
    pub(super) fn dual_repaired(&mut self, result: Result<(), String>) {
        self.dual.action = None;
        if result.as_ref().err().map(String::as_str) == Some(account::NoOwner.to_string().as_str())
        {
            self.owner = Some(false);
            self.owner_screen_open = true;
            self.dual.notice = Some(account::NoOwner::in_window());
        } else {
            self.dual.notice = Some(
                result
                    .err()
                    .unwrap_or_else(|| tr("Game files verified").into()),
            );
        }
    }

    fn dual_download_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        button(
            "dual-direct",
            "Download Minecraft",
            Kind::Primary,
            &self.theme,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.open_direct_install(cx);
            cx.notify();
        }))
    }

    pub(super) fn dual_page(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let selected = self.settings.bodka.dual_source;
        let started = self.dual.generation != 0;
        let row = || {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .when(i18n::is_rtl(), |d| d.flex_row_reverse())
        };
        let muted = |label: String| {
            ui::text(14., FontWeight::NORMAL, 0.85)
                .when(i18n::is_rtl(), |d| d.text_right())
                .child(label)
        };
        let mut choices = row();
        for (id, source, label) in [
            ("mc", Source::Official, "Minecraft Launcher"),
            ("tl", Source::Tlauncher, "TLauncher"),
            ("any", Source::Auto, "All local sources"),
        ] {
            choices = choices.child(
                button(
                    format!("dual-source-{id}"),
                    label,
                    if started && selected == source {
                        Kind::Primary
                    } else {
                        Kind::Secondary
                    },
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.select_dual_source(source);
                    cx.notify();
                })),
            );
        }
        let mut page = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(14.))
            .child(div().flex().flex_col().gap(px(6.)).pt(px(16.))
                .when(i18n::is_rtl(), |d| d.text_right())
                .child(ui::text(32., FontWeight::SEMIBOLD, 1.).child(tr("Use an installed launcher")))
                .child(muted(tr("Choose where you play. BODKA will check for local versions and their mods.").into())))
            .child(choices);
        if !started {
            return page.child(self.dual_download_button(cx));
        }
        let mut tools = row().child(
            button("dual-folder", "Choose game folder", Kind::Secondary, &t)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dual_folder(cx))),
        );
        if !self.dual.scanning() {
            tools = tools.child(
                button("dual-all", "Search all drives", Kind::Ghost, &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.set_dual_folder(None);
                        cx.notify();
                    },
                )),
            );
        }
        let installed = self
            .dual
            .summary
            .as_ref()
            .and_then(|summary| summary.installed.iter().find(|app| app.source == selected));
        if installed.is_some() {
            let label = if selected == Source::Tlauncher {
                "Open TLauncher"
            } else {
                "Open Minecraft Launcher"
            };
            tools = tools.child(button("dual-open-launcher", label, Kind::Secondary, &t)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(app) = this.dual.summary.as_ref().and_then(|s| s.installed.iter().find(|a| a.source == selected)) {
                        if app.open().is_err() { this.dual.notice = Some(tr("Windows could not open this launcher. Open it from Start or select its game folder.").into()); }
                    }
                    cx.notify();
                })));
        }
        if selected == Source::Tlauncher && self.dual.summary.is_some() && installed.is_none() {
            page = page.child(crate::glass::plate(12.).flex().flex_col().p(px(16.)).gap(px(8.))
                .when(i18n::is_rtl(), |d| d.text_right())
                .child(ui::text(17., FontWeight::SEMIBOLD, 1.).child(tr("TLauncher was not detected on this PC")))
                .child(muted(tr("If you want TLauncher, download it from its publisher, install it on Windows, then return here and check again. For a portable installation, select its game folder.").into()))
                .child(row()
                    .child(button("tl-site", "Open the TLauncher website", Kind::Secondary, &t)
                        .on_click(|_,_,cx| cx.open_url("https://tlauncher.org/en/")))
                    .child(button("tl-recheck", "Check again", Kind::Ghost, &t)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.scan_dual(false);cx.notify();
                        })))));
        }
        if let Some(folder) = &self.settings.bodka.dual_folder {
            page = page.child(muted(folder.display().to_string()));
        }
        page = page.child(tools);
        if self.dual.scanning() {
            page = page.child(muted(tr(if selected == Source::Tlauncher {
                "Checking for TLauncher and its versions…"
            } else { "Scanning local installations…" }).into()));
        }
        if let Some(summary) = &self.dual.summary {
            if summary.volumes > 0 {
                page = page.child(muted(trf(
                    "Drives scanned: {drives} · Inaccessible folders: {skipped}",
                    &[
                        ("drives", &summary.volumes.to_string()),
                        ("skipped", &summary.skipped.to_string()),
                    ],
                )));
            }
        }
        if let Some(action) = self.dual.action {
            page = page.child(muted(
                tr(if action == Action::Repair {
                    "Checking game files…"
                } else {
                    "Importing local instance…"
                })
                .into(),
            ));
        }
        if let Some(status) = &self.dual.notice {
            page = page.child(notice(status.clone(), &t));
        }
        let filtered = self
            .dual
            .rows
            .iter()
            .enumerate()
            .filter(|(_, e)| e.matches(self.settings.bodka.dual_source));
        let count = filtered.clone().count();
        if count == 0 && !self.dual.scanning() {
            let guidance = if self.settings.bodka.dual_folder.is_some() {
                "Choose the game folder that contains versions, or download a new version with BODKA."
            } else if installed.is_some() {
                "The launcher is installed. Install a Java version in it, then select its source again to scan."
            } else {
                match selected {
                    Source::Tlauncher => "No TLauncher versions found. If you use TLauncher, open its folder icon and select that game folder here.",
                    Source::Official => "No Minecraft Launcher versions found. Open Java Edition in the launcher or download a version directly with BODKA.",
                    Source::Auto => "Choose a game folder, search all drives, or let BODKA download Minecraft.",
                }
            };
            page = page.child(
                crate::glass::plate(12.)
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .p(px(18.))
                    .child(
                        ui::text(18., FontWeight::SEMIBOLD, 1.)
                            .child(tr("No local versions found in this source")),
                    )
                    .child(muted(tr(guidance).into()))
                    .child(self.dual_download_button(cx)),
            );
        }
        for (index, e) in filtered.take(self.dual.limit) {
            let bound =
                self.dual.bindings.iter().any(|(o, id)| {
                    o == &e.origin && self.instances.list.iter().any(|i| &i.id == id)
                });
            let mut tile = crate::glass::plate(8.)
                .flex()
                .flex_col()
                .gap(px(8.))
                .p(px(16.))
                .child(ui::text(17., FontWeight::SEMIBOLD, 1.).child(e.name.clone()))
                .child(muted(format!(
                    "{}  ·  {}{}",
                    tr(e.label()),
                    e.minecraft,
                    e.loader
                        .as_ref()
                        .map(|l| format!("  ·  {} {}", l.kind.name(), l.version))
                        .unwrap_or_default()
                )))
                .child(muted(e.folder().display().to_string()));
            let mut actions = row();
            if !e.mods.is_empty() {
                let label = trf(
                    "Local mods ({count})",
                    &[("count", &e.mods.len().to_string())],
                );
                let origin = e.origin.clone();
                actions = actions.child(
                    button(format!("dual-mods-{index}"), label, Kind::Ghost, &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.dual.expanded = if this.dual.expanded.as_ref() == Some(&origin) {
                                None
                            } else {
                                Some(origin.clone())
                            };
                            cx.notify();
                        }),
                    ),
                );
                if self.dual.expanded.as_ref() == Some(&e.origin) {
                    tile = tile.child(muted(
                        e.mods
                            .iter()
                            .filter_map(|p| p.file_name())
                            .map(|n| n.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("  ·  "),
                    ));
                }
            } else {
                tile = tile.child(muted(tr("No local mods").into()));
            }
            if !e.supported {
                tile = tile.child(muted(tr("Unsupported local profile").into()));
            } else if self.dual.action.is_none()
                && self.client.is_none()
                && self.starting_client.is_none()
                && !self.preparing
            {
                for (key, label, action, kind) in [
                    (
                        "play",
                        if bound {
                            "Play game"
                        } else {
                            "Import and play"
                        },
                        Action::Play,
                        Kind::Primary,
                    ),
                    ("import", "Import instance", Action::Import, Kind::Secondary),
                    (
                        "repair",
                        "Repair game files",
                        Action::Repair,
                        Kind::Secondary,
                    ),
                ] {
                    if action == Action::Import && bound {
                        continue;
                    }
                    actions = actions.child(
                        button(format!("dual-{key}-{index}"), label, kind, &t).on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.dual_action(index, action);
                                cx.notify();
                            }),
                        ),
                    );
                }
            }
            page = page.child(tile.child(actions));
        }
        if count > self.dual.limit {
            page = page.child(
                button("dual-more", "Show more", Kind::Secondary, &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.dual.limit += 20;
                        cx.notify();
                    },
                )),
            );
        }
        page
    }
}

#[cfg(all(test, windows, feature = "visual-tests"))]
#[test]
#[ignore = "opens the Windows launcher in a fresh guest profile and uses Mojang's public catalog"]
fn empty_source_to_direct_download_on_windows() {
    use gpui::AnyWindowHandle;
    let scratch = PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("isolated profile"));
    assert!(
        backend::data_dir().starts_with(&scratch) && backend::game_home().starts_with(&scratch)
    );
    let empty = scratch.join("empty-source");
    std::fs::create_dir_all(&empty).unwrap();
    let completed = Arc::new(AtomicBool::new(false));
    let success = completed.clone();
    gpui_platform::application().with_assets(blocks::Assets).run(move |cx| {
        let handle = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1120.), px(740.)), cx))),
            show: true, focus: false, ..Default::default()
        }, |w,cx| cx.new(|cx| {
            let mut app=Launcher::new(w,cx);
            app.collection.consent.answer(Default::default());
            app.opening=None;
            app.show_page(Page::Dual);
            app.settings.bodka.dual_source=Source::Tlauncher;
            app.set_dual_folder(Some(empty));
            app
        })).unwrap();
        cx.spawn(async move |cx| {
            let mut stage=0;
            for _ in 0..320 {
                cx.background_executor().timer(Duration::from_millis(125)).await;
                let ready=handle.update(cx, |app,_,_| match stage {
                    0 => app.dual.summary.is_some() && app.dual.rows.is_empty(),
                    1 => matches!(&app.instances.versions, new_instance::VersionList::Ready(v) if app.draft_minecraft().as_deref() == Some(&v.latest_release)),
                    _ => app.instances.dialog.is_none() && app.asking_owner()
                        && matches!(&app.instances.versions, new_instance::VersionList::Ready(v) if app.pending_java_version() == Some(v.latest_release.as_str())),
                }).unwrap();
                if !ready {continue;}
                let any:AnyWindowHandle=handle.into();
                let image=cx.update(|cx| any.update(cx, |_,w,cx| {
                    w.refresh();
                    w.draw(cx).clear(cx);
                    w.render_to_image().unwrap()
                }).unwrap());
                image.save(scratch.join(["empty-source.png","direct-download.png","link-account.png"][stage])).unwrap();
                if stage == 2 {
                    handle.update(cx, |app, _, _| {
                        use libraries::JavaAction;
                        let id = app.selected_instance().unwrap().id.clone();
                        assert_eq!(app.library.pending_action(), Some(&(id.clone(), JavaAction::Download)));
                        for (destination, action) in [
                            (None, JavaAction::Play),
                            (Some(("--world", "My world".to_string())), JavaAction::World("My world".into())),
                            (Some(("--server", "localhost".to_string())), JavaAction::Server("localhost".into())),
                        ] {
                            app.library.cancel_login_action();
                            assert!(app.pending_java_version().is_none());
                            app.quick_play = destination;
                            app.play();
                            assert_eq!(app.library.pending_action(), Some(&(id.clone(), action)));
                            assert!(app.asking_owner() && app.starting_client.is_none());
                        }
                        app.library.cancel_login_action();
                    }).unwrap();
                    success.store(true, Ordering::Relaxed);
                    break;
                }
                handle.update(cx, |app,_,cx| {
                    if stage==0 {app.open_direct_install(cx);} else {app.save_instance(cx);}
                    cx.notify();
                }).unwrap();
                stage+=1;
            }
            cx.update(|cx| cx.quit());
        }).detach();
    });
    assert!(
        completed.load(Ordering::Relaxed),
        "empty directory → current Mojang release → saved library → ownership prompt"
    );
}
