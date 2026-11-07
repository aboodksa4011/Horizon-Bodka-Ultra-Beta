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

impl Drop for Catalog {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl Launcher {
    pub(super) fn scan_dual(&mut self) {
        let state = &mut self.dual;
        if let Some(old) = state.cancel.take() {
            old.store(true, Ordering::Relaxed);
        }
        state.generation += 1;
        state.rows.clear();
        state.summary = None;
        state.expanded = None;
        state.limit = 20;
        let generation = state.generation;
        let manual = self.settings.bodka.dual_folder.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        state.cancel = Some(cancel.clone());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::DualBindings(generation, dual::imported()));
            let summary = dual::discover(manual, &cancel, |entries| {
                let _ = tx.send(Msg::DualFound(generation, entries));
            });
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
        self.settings.bodka.dual_source = Source::Auto;
        self.save_settings();
        self.scan_dual();
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
                self.select_instance(&instance);
                self.go(Page::Play);
                self.start_java(instance, None);
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

    pub(super) fn dual_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.dual.generation == 0 {
            self.scan_dual();
        }
        let t = self.theme;
        let row = || {
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .when(i18n::is_rtl(), |d| d.flex_row_reverse())
        };
        let muted = |label: String| ui::text(13., FontWeight::NORMAL, 0.73).child(label);
        let mut controls = row();
        for (i, source) in [Source::Auto, Source::Official, Source::Tlauncher]
            .into_iter()
            .enumerate()
        {
            let selected = source == self.settings.bodka.dual_source;
            controls = controls.child(
                button(
                    format!("dual-source-{i}"),
                    source.label(),
                    if selected { Kind::Primary } else { Kind::Ghost },
                    &t,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.settings.bodka.dual_source = source;
                    this.dual.limit = 20;
                    this.save_settings();
                    cx.notify();
                })),
            );
        }
        let mut paths = row().child(
            button("dual-folder", "Choose game folder", Kind::Secondary, &t)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.dual_folder(cx))),
        );
        if self.settings.bodka.dual_folder.is_some() {
            paths = paths.child(
                button("dual-all", "Search all drives", Kind::Secondary, &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.set_dual_folder(None);
                        cx.notify();
                    },
                )),
            );
        }
        if self.dual.cancel.is_none() {
            paths = paths.child(
                button("dual-rescan", "Scan again", Kind::Ghost, &t).on_click(cx.listener(
                    |this, _: &ClickEvent, _, cx| {
                        this.scan_dual();
                        cx.notify();
                    },
                )),
            );
        }
        let mut page = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.))
            .child(ui::page_head(
                tr("BODKA Dual Launcher"),
                tr("Discover local Java versions and import their mods into a separate instance."),
            ))
            .child(controls)
            .child(paths);
        if let Some(folder) = &self.settings.bodka.dual_folder {
            page = page.child(muted(folder.display().to_string()));
        }
        if self.dual.cancel.is_some() {
            page = page.child(muted(tr("Scanning local installations…").into()));
        }
        if let Some(summary) = &self.dual.summary {
            if self.settings.bodka.dual_folder.is_none() {
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
        if count == 0 && self.dual.cancel.is_none() {
            page = page.child(muted(tr("No local versions found in this source").into()));
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
