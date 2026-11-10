//! Libraries are existing Horizon instances. Navigation and transient progress live in the window.
use super::*;
use crate::glass_ui as ui;
use horizon_launcher::browser::{shared_library::Profile, Kind as Content};
use instances::{Instance, Kind as InstanceKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum JavaAction {
    Download,
    Play,
    World(String),
    Server(String),
}

#[derive(Default)]
pub(super) struct Workspace {
    history: Vec<Page>,
    statuses: HashMap<String, Result<String, String>>,
    readiness: HashMap<String, (String, Instant, bool)>,
    horizon_queue: VecDeque<(String, String)>,
    preparing_horizon: Option<String>,
    review: Option<Profile>,
    difference: Option<usize>,
    comparison_generation: u64,
    sharing: bool,
    share_note: Option<String>,
    pub sidebar_tools: bool,
    queued_join: Option<(String, String)>,
    game_session: Option<(String, Option<String>)>,
    missing_mods: bool,
    pending_auth: Option<(String, JavaAction)>,
    requested_play: Option<(String, JavaAction)>,
}
impl Workspace {
    pub fn visit(&mut self, old: Page, new: Page) {
        if let Some(index) = self.history.iter().position(|p| *p == new) {
            self.history.truncate(index);
        } else {
            self.history.push(old);
            if self.history.len() > 12 {
                self.history.remove(0);
            }
        }
        if new == Page::Dashboard {
            self.history.clear();
        }
    }
    pub fn browsing_library(&self) -> bool {
        self.history.last() == Some(&Page::Library)
    }
    pub fn end_java_session(&mut self) {
        self.game_session = None;
        self.missing_mods = false;
    }
    pub fn begin_game(&mut self, id: &str, server: Option<&str>) {
        self.game_session = Some((id.to_owned(), server.map(str::to_owned)));
        self.missing_mods = false;
    }
    pub fn watch_client_line(&mut self, line: &str) {
        if self.game_session.is_some() && !self.missing_mods && missing_mod_message(line) {
            self.missing_mods = true;
            glass_toast::say(tr("Minecraft reported incompatible or missing mods."));
        }
    }
    pub fn cancel_login_action(&mut self) {
        self.pending_auth = None;
        self.queued_join = None;
    }
    pub fn pending_action(&self) -> Option<&(String, JavaAction)> {
        self.pending_auth.as_ref()
    }

    pub fn needs_link(&mut self, id: String, action: JavaAction) {
        self.pending_auth = Some((id, action));
    }
    pub fn take_requested_play(&mut self, id: &str) -> Option<JavaAction> {
        if self.requested_play.as_ref()?.0 == id {
            self.requested_play.take().map(|(_, action)| action)
        } else {
            None
        }
    }
    pub fn busy(&self) -> bool {
        self.sharing || self.statuses.values().any(Result::is_ok) || !self.horizon_queue.is_empty()
    }
    pub fn progress(&mut self, id: String, line: String) {
        self.statuses.insert(id, Ok(line));
    }
    pub fn finished(&mut self, id: String, result: Result<(), String>) {
        self.readiness.remove(&id);
        match result {
            Ok(()) => {
                self.statuses.remove(&id);
                glass_toast::say(tr("Library download complete"));
            }
            Err(e) => {
                self.statuses.insert(id, Err(e));
            }
        }
    }
    pub fn horizon_finished(&mut self, result: &Result<(), String>) {
        if let Some(id) = self.preparing_horizon.take() {
            self.finished(id, result.clone());
        }
    }
}

fn missing_mod_message(line: &str) -> bool {
    if line.len() > 16384 {
        return false;
    }
    let message = line.to_ascii_lowercase();
    [
        "net.minecraftforge.fml.loading.earlyloadingexception: missing",
        "missing mandatory dependencies",
        "incompatible mod set!",
        "incompatible mods found!",
        "mod resolution encountered an incompatible mod set",
        "mismatched mod channel list",
        "channels rejected their server side version",
        "failed to synchronize registry data from server",
        "requires the following mods to run",
        "missing or unsupported mandatory dependencies",
    ]
    .iter()
    .any(|mark| message.contains(mark))
        || (message.contains("requires version")
            && (message.contains("which is missing")
                || message.contains("but only the wrong version is present")))
}
fn line() -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .when(i18n::is_rtl(), |d| d.flex_row_reverse())
}
fn title(s: impl Into<SharedString>, sz: f32) -> Div {
    ui::text(sz, FontWeight::SEMIBOLD, 1.)
        .when(i18n::is_rtl(), |d| d.text_right())
        .child(s.into())
}
impl Launcher {
    pub(super) fn prepare_new_library(&mut self, i: &Instance, _cx: &mut Context<Self>) {
        if i.java.is_some() {
            if self.owner == Some(true) {
                self.prepare_java_library(i.clone());
            }
        } else if let Some(tag) = i
            .version
            .clone()
            .or_else(|| self.launcher_version().cloned())
            .or_else(|| self.horizon_choices().into_iter().next())
        {
            if !backend::is_prepared(&tag)
                && !self.library.horizon_queue.iter().any(|(id, _)| id == &i.id)
            {
                self.library.horizon_queue.push_back((i.id.clone(), tag));
            }
        }
    }
    pub(super) fn pending_java_version(&self) -> Option<&str> {
        let (id, _) = self.library.pending_action()?;
        self.instances.list.iter().find(|i| &i.id == id)?.java.as_ref()
            .map(|game| game.minecraft.as_str())
    }

    pub(super) fn request_java_action(&mut self, instance: &Instance, action: JavaAction) {
        self.library.pending_auth = Some((instance.id.clone(), action));
        if self.owner != Some(true) {
            self.owner_screen_open = true;
        }
        self.resume_java_action();
    }

    fn resume_java_action(&mut self) {
        if self.owner != Some(true)
            || self.client.is_some()
            || self.starting_client.is_some()
            || self.preparing
        {
            return;
        }
        let Some((id, action)) = self.library.pending_auth.take() else {
            return;
        };
        let Some(instance) = self.instances.list.iter().find(|i| i.id == id).cloned() else {
            return;
        };
        self.owner_screen_open = false;
        match action {
            JavaAction::Download => self.prepare_java_library(instance),
            play => {
                self.select_instance(&instance);
                self.go(Page::Play);
                self.library.requested_play = Some((id, play.clone()));
                match play {
                    JavaAction::World(folder) => self.start_java_at(instance, None, Some(folder)),
                    JavaAction::Server(address) => self.start_java_at(instance, Some(address), None),
                    _ => self.start_java_at(instance, None, None),
                }
            }
        }
    }

    pub(super) fn next_library_preparation(&mut self) {
        self.resume_java_action();
        if self.preparing || self.client.is_some() || self.starting_client.is_some() {
            return;
        }
        if let Some((id, address)) = self.library.queued_join.take() {
            if let Some(i) = self.instances.list.iter().find(|i| i.id == id).cloned() {
                self.request_java_action(&i, JavaAction::Server(address));
            }
            return;
        }
        let Some((id, tag)) = self.library.horizon_queue.front().cloned() else {
            return;
        };
        if !self.installed.contains(&tag) {
            if self.installing.is_some() {
                return;
            }
            if self.install_error.as_ref().is_some_and(|(t, _)| t == &tag) {
                self.library.horizon_queue.pop_front();
                self.library
                    .finished(id, Err(self.install_error.as_ref().unwrap().1.clone()));
                return;
            }
            if let Some(release) = self.release_to_install(&tag) {
                self.install_for_instance(release);
                return;
            }
            if matches!(self.releases, Load::Loading) {
                return;
            }
            self.library.horizon_queue.pop_front();
            self.library
                .finished(id, Err(tr("No version installed").into()));
            return;
        }
        self.library.horizon_queue.pop_front();
        if backend::is_prepared(&tag) {
            self.library.finished(id, Ok(()));
            return;
        }
        self.library.preparing_horizon = Some(id.clone());
        self.library.progress(id, tr("Preparing game files").into());
        self.prepare_tag(
            tag,
            if self.owner == Some(true) {
                backend::MinecraftFrom::Mojang
            } else {
                backend::MinecraftFrom::Install
            },
        );
    }
    fn prepare_java_library(&mut self, i: Instance) {
        if self.library.statuses.get(&i.id).is_some_and(Result::is_ok) {
            return;
        }
        self.library
            .progress(i.id.clone(), tr("Preparing game files").into());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = backend::install_java_library(&i, &|line| {
                let _ = tx.send(Msg::LibraryProgress(i.id.clone(), line));
            });
            let _ = tx.send(Msg::LibraryPrepared(i.id, result));
        });
    }
    fn library_ready(&mut self, i: &Instance) -> bool {
        let version = i
            .java
            .as_ref()
            .map(instances_page::java_line)
            .or_else(|| self.version_of(i).cloned())
            .unwrap_or_default();
        if let Some((key, time, value)) = self.library.readiness.get(&i.id) {
            if *key == version && time.elapsed() < Duration::from_secs(3) {
                return *value;
            }
        }
        let value = i.java.as_ref().map_or_else(
            || !version.is_empty() && backend::is_prepared(&version),
            backend::java_library_installed,
        );
        self.library
            .readiness
            .insert(i.id.clone(), (version, Instant::now(), value));
        value
    }

    pub(super) fn library_home(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let direct = button("library-direct", "Download Minecraft", Kind::Primary, &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.open_direct_install(cx);
                cx.notify();
            }),
        );
        let import = button(
            "library-local",
            "Use an installed launcher",
            Kind::Secondary,
            &t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.go(Page::Dual);
            cx.notify();
        }));
        let create = button("library-create", "Create library", Kind::Ghost, &t).on_click(
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.library.review = None;
                this.library.share_note = None;
                this.open_java_library(cx);
                cx.notify();
            }),
        );
        let libraries: Vec<_> = self
            .instances
            .list
            .iter()
            .filter(|i| {
                i.kind != InstanceKind::Default
                    || i.last_played.is_some()
                    || self.instances.default_has_worlds
            })
            .cloned()
            .collect();
        if libraries.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .min_h(px(375.))
                .w_full()
                .child(
                    crate::glass::plate(18.)
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(22.))
                        .min_w(px(262.))
                        .p(px(30.))
                        .border_1()
                        .border_color(t.accent.opacity(0.24))
                        .child(ui::icon("archive", 38., t.accent))
                        .child(title(tr("You have no libraries yet"), 25.))
                        .child(ui::text(14., FontWeight::NORMAL, 0.84).child(tr(
                            "Download from Mojang or bring a version you already play.",
                        )))
                        .child(line().justify_center().child(direct).child(import))
                        .child(create),
                );
        }
        let mut cards = line().items_start();
        for i in libraries {
            cards = cards.child(self.library_card(i, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .py(px(22.))
            .child(
                line()
                    .justify_between()
                    .child(title(tr("My libraries"), 30.))
                    .child(line().child(direct).child(import).child(create)),
            )
            .child(cards)
    }

    fn library_card(&mut self, i: Instance, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let pack = matches!(i.kind, InstanceKind::Pack | InstanceKind::Port);
        let ready = self.library_ready(&i);
        let java = i.java.is_some();
        let version = i
            .java
            .as_ref()
            .map(instances_page::java_line)
            .unwrap_or_else(|| {
                let tag = i.version.as_ref().or_else(|| self.version_of(&i));
                tag.map_or_else(|| tr("Horizon Rust").into(), |v| format!("Horizon · {v}"))
            });
        let job = self.library.statuses.get(&i.id);
        let tag = i.version.as_ref().or_else(|| self.version_of(&i));
        let download = self
            .installing
            .as_ref()
            .filter(|(current, _)| !java && Some(current) == tag);
        let queued = self.library.horizon_queue.iter().any(|(id, _)| id == &i.id);
        let busy = job.is_some_and(Result::is_ok) || download.is_some() || queued;
        let status = match (download, job) {
            (Some((_, fraction)), _) => format!(
                "{} · {:.0}%",
                tr("Downloading game files"),
                fraction.clamp(0., 1.) * 100.
            ),
            (_, Some(Ok(line) | Err(line))) => line.clone(),
            _ if queued => tr("Preparing game files").into(),
            _ if pack => tr("Modpack").into(),
            _ if ready => tr("Installed").into(),
            _ if java && self.owner == Some(false) => tr("Sign in to play").into(),
            _ => tr("Needs preparing").into(),
        };
        let failed = job.is_some_and(Result::is_err);
        let icon = self.instance_icon(&i, 52.);
        let edit = i.clone();
        let play = i.clone();
        let label = if pack && !ready {
            "Modpack"
        } else if ready {
            "Play"
        } else {
            "Download and install"
        };
        let action = button(format!("library-play-{}", i.id), label, Kind::Primary, &t);
        let blocked =
            busy || self.client.is_some() || self.starting_client.is_some() || self.preparing;
        let action = if blocked {
            action.opacity(0.55).cursor_default()
        } else {
            action.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.select_instance(&play);
                if pack && ready {
                    this.play_pack(play.work.clone());
                } else if pack {
                    this.open_library(&play, cx);
                } else if java {
                    this.request_java_action(
                        &play,
                        if ready {
                            JavaAction::Play
                        } else {
                            JavaAction::Download
                        },
                    );
                } else if ready {
                    this.go(Page::Play);
                    this.play();
                } else {
                    this.install_error = None;
                    this.prepare_new_library(&play, cx);
                    if this.library.horizon_queue.is_empty() {
                        this.go(Page::Versions);
                    }
                }
                cx.notify();
            }))
        };
        crate::glass::plate(14.)
            .w(px(294.))
            .max_w_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(18.))
            .border_1()
            .border_color(t.accent.opacity(0.23))
            .child(
                line().child(icon).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(title(i.name, 19.).truncate())
                        .child(title(version, 12.).text_color(t.muted)),
                ),
            )
            .child(
                ui::text(13., FontWeight::NORMAL, 0.88)
                    .when(i18n::is_rtl(), |d| d.text_right())
                    .text_color(if failed { t.danger } else { t.accent })
                    .child(status),
            )
            .child(
                line().child(action).child(
                    button(
                        format!("library-edit-{}", edit.id),
                        "Manage library",
                        Kind::Secondary,
                        &t,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.open_library(&edit, cx);
                            cx.notify();
                        },
                    )),
                ),
            )
    }

    fn open_library(&mut self, i: &Instance, cx: &mut Context<Self>) {
        if i.kind == InstanceKind::Port {
            self.show_modpack(i.id.strip_prefix("port:").map(str::to_owned));
            return;
        }
        if i.kind == InstanceKind::Pack {
            self.show_modpack(None);
            return;
        }
        self.select_instance(i);
        self.mods_load();
        self.library.review = None;
        self.library.share_note = None;
        self.home_tab = if self.has_mods_tab() {
            home_tabs::HomeTab::Mods
        } else {
            home_tabs::HomeTab::Worlds
        };
        self.go(Page::Library);
        cx.notify();
    }

    pub(super) fn library_back(&self, cx: &mut Context<Self>) -> Div {
        let back = self
            .library
            .history
            .last()
            .copied()
            .unwrap_or(Page::Dashboard);
        div().flex().justify_end().child(
            button("library-back", "Back", Kind::Secondary, &self.theme).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.go(back);
                    cx.notify();
                },
            )),
        )
    }
    pub(super) fn mod_mismatch_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.library.missing_mods {
            return None;
        }
        let (id, address) = self.library.game_session.clone()?;
        let i = self.instances.list.iter().find(|i| i.id == id)?.clone();
        Some(
            crate::glass::plate(10.).p(px(16.)).child(
                line()
                    .justify_between()
                    .child(
                        title(tr("Minecraft reported incompatible or missing mods."), 15.)
                            .text_color(self.theme.warn),
                    )
                    .child(
                        button(
                            "library-repair-friend",
                            "Compare a friend's library",
                            Kind::Primary,
                            &self.theme,
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.open_library(&i, cx);
                                this.servers
                                    .text(servers_page::Field::LibraryJoin)
                                    .set(address.clone().unwrap_or_default());
                                this.library.share_note = Some(
                                tr("Paste your friend's BODKA code to check the required files.")
                                    .into(),
                            );
                                cx.notify();
                            },
                        )),
                    ),
            ),
        )
    }
    pub(super) fn library_editor(&mut self, cx: &mut Context<Self>) -> Div {
        let t = self.theme;
        let Some(i) = self.selected_instance().cloned() else {
            return self.library_home(cx);
        };
        self.mods_load();
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(
                ui::quiet_icon("library-add", Some("plus-sign"), tr("Add content"), 36., &t)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.mods_load();
                        this.mods.everything = false;
                        let kind = if this.home_tab == home_tabs::HomeTab::Packs {
                            Content::ResourcePack
                        } else {
                            Content::Mod
                        };
                        this.mods_set_kind(kind);
                        this.go(Page::Mods);
                        cx.notify();
                    })),
            )
            .child(self.library_back(cx));
        header = header.py(px(10.));
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(header)
            .child(
                line()
                    .child(self.instance_icon(&i, 46.))
                    .child(title(i.name.clone(), 26.)),
            )
            .child(self.glass_home_tabs(div(), cx))
    }

    pub(super) fn library_actions(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        let i = self.selected_instance()?.clone();
        let mut body = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_t_1()
            .border_color(gpui::white().opacity(0.12))
            .p(px(10.));
        let edit = i.clone();
        let copy = i.clone();
        let java_mods = i.java.as_ref().is_some_and(|j| j.loader.is_some());
        body = body.child(
            line()
                .when(java_mods, |d| {
                    d.child(
                        ui::quiet(
                            "library-essential",
                            tr("Essential for this library"),
                            32.,
                            &t,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.open_essential_catalog();
                                cx.notify();
                            },
                        )),
                    )
                })
                .child(
                    ui::quiet("library-files", tr("Choose files…"), 32., &t).on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.choose_library_files(cx);
                        },
                    )),
                )
                .child(
                    ui::quiet("library-paths", tr("Paste file paths"), 32., &t).on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            let data = horizon_launcher::desktop_seams::clipboard_file()
                                .and_then(|p| std::fs::read_to_string(p).ok())
                                .or_else(|| cx.read_from_clipboard().and_then(|c| c.text()))
                                .unwrap_or_default();
                            let paths: Vec<std::path::PathBuf> = data
                                .lines()
                                .take(64)
                                .map(|s| std::path::PathBuf::from(s.trim().trim_matches('"')))
                                .filter(|p| p.is_absolute() && p.is_file())
                                .collect();
                            if paths.is_empty() {
                                this.library.share_note = Some(
                                    tr("Use a library code or copy file paths from Explorer.")
                                        .into(),
                                );
                            } else {
                                this.import_dropped(paths, cx);
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    ui::quiet("library-settings", tr("Library settings"), 32., &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_instance_dialog(Some(edit.clone()));
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    ui::quiet("library-share", tr("Copy library code"), 32., &t).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            match Profile::of(&copy, this.version_of(&copy).map(String::as_str))
                                .and_then(|p| p.code())
                            {
                                Ok(code) => {
                                    copy_to_clipboard(cx, gpui::ClipboardItem::new_string(code));
                                    glass_toast::say(tr("Library code copied"));
                                    this.library.share_note = None;
                                }
                                Err(e) => this.library.share_note = Some(e),
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(self.library_paste_button(cx))
                .when(self.owner != Some(true), |d| {
                    d.child(
                        ui::quiet(
                            "library-link-microsoft",
                            tr("Link Microsoft via Horizon"),
                            32.,
                            &t,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.owner_screen_open = true;
                                cx.notify();
                            },
                        )),
                    )
                })
                .child(
                    ui::quiet("library-export", tr("Export"), 32., &t).on_click(cx.listener(
                        |this, _: &ClickEvent, _, cx| {
                            this.toggle_export();
                            cx.notify();
                        },
                    )),
                ),
        );
        if let Some(export) = self.export_panel(cx) {
            body = body.child(export);
        }
        if let Some(review) = self.library_review(cx) {
            body = body.child(review);
        }
        Some(body)
    }

    fn choose_library_files(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(tr("Choose files…").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(files))) = paths.await {
                let _ = this.update(cx, |this, cx| {
                    this.import_dropped(files, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }
    pub(super) fn library_paste_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        button(
            "library-paste",
            "Read library code",
            Kind::Secondary,
            &self.theme,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            let data = horizon_launcher::desktop_seams::clipboard_file()
                .and_then(|p| std::fs::read_to_string(p).ok())
                .or_else(|| cx.read_from_clipboard().and_then(|c| c.text()))
                .unwrap_or_default();
            match Profile::parse(&data) {
                Ok(p) => {
                    this.library.comparison_generation += 1;
                    let generation = this.library.comparison_generation;
                    this.library.difference = None;
                    let candidate = p.clone();
                    let instance = this.selected_instance().cloned();
                    let task = cx.background_executor().spawn(async move {
                        instance.map_or(candidate.projects.len(), |i| candidate.missing(&i))
                    });
                    cx.spawn(async move |this, cx| {
                        let count = task.await;
                        let _ = this.update(cx, |this, cx| {
                            if this.library.comparison_generation == generation {
                                this.library.difference = Some(count);
                                cx.notify();
                            }
                        });
                    })
                    .detach();
                    this.library.review = Some(p);
                    this.library.share_note = None;
                }
                Err(e) => {
                    this.library.review = None;
                    this.library.share_note = Some(e);
                }
            }
            cx.notify();
        }))
    }
    pub(super) fn library_review(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let t = self.theme;
        let mut body = div().flex().flex_col().gap(px(8.));
        if let Some(note) = &self.library.share_note {
            body = body.child(title(note.clone(), 14.));
        }
        let Some(profile) = self.library.review.clone() else {
            return self.library.share_note.is_some().then_some(body);
        };
        let count = profile.projects.len().to_string();
        let missing = self
            .library
            .difference
            .map_or_else(|| "…".into(), |n| n.to_string());
        let loader = profile.loader.as_ref().map_or(
            if profile.horizon {
                "Horizon Rust"
            } else {
                "Vanilla"
            },
            |l| l.kind.name(),
        );
        body = body
            .child(title(
                format!("{} · {} · {}", profile.name, profile.minecraft, loader),
                17.,
            ))
            .child(title(
                trf(
                    "Shared files: {count} · Missing or different: {missing}",
                    &[("count", &count), ("missing", &missing)],
                ),
                14.,
            ));
        if let Some(i) = self
            .selected_instance()
            .filter(|i| i.kind != InstanceKind::Default)
        {
            let matches = i.java.as_ref().map_or(
                profile.horizon && self.version_of(i) == Some(&profile.minecraft),
                |game| {
                    !profile.horizon
                        && game.minecraft == profile.minecraft
                        && game.loader == profile.loader
                },
            );
            if !matches {
                body = body.child(title(tr("Game engine differs"), 14.).text_color(t.warn));
            } else if missing == "0" {
                body = body.child(title(tr("Required files match"), 14.).text_color(t.success));
            }
        }
        let join_profile = profile.clone();
        let action = button("library-import-code", "Create from code", Kind::Primary, &t);
        let busy = self.library.sharing;
        body = body.child(if busy {
            action.opacity(0.5)
        } else {
            action.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.import_library_profile(profile.clone(), None, cx);
                cx.notify();
            }))
        });
        if !join_profile.horizon && self.page == Page::Library {
            let field = self.field(
                servers_page::Field::LibraryJoin,
                tr("Friend's server address"),
                260.,
                cx,
            );
            let address = self
                .servers
                .value(servers_page::Field::LibraryJoin)
                .trim()
                .to_string();
            let valid = !address.is_empty()
                && address.len() <= 255
                && address
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".:-[]".contains(&c));
            let join = button("library-sync-join", "Install and join", Kind::Primary, &t);
            body = body.child(line().child(field).child(if busy || !valid {
                join.opacity(0.45)
            } else {
                join.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.import_library_profile(join_profile.clone(), Some(address.clone()), cx);
                    cx.notify();
                }))
            }));
        }
        Some(body)
    }
    fn import_library_profile(
        &mut self,
        profile: Profile,
        join: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.library.sharing {
            return;
        }
        self.library.sharing = true;
        self.library.share_note = Some(tr("Downloading game files").into());
        let task = cx
            .background_executor()
            .spawn(async move { profile.import(&|_, _| {}) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.library.sharing = false;
                match result {
                    Ok(i) => {
                        this.library.review = None;
                        this.library.share_note = None;
                        this.instances.dialog = None;
                        this.reload_instances();
                        this.select_instance(&i);
                        this.go(Page::Dashboard);
                        if let Some(address) = join {
                            this.library.queued_join = Some((i.id.clone(), address));
                        } else {
                            this.prepare_new_library(&i, cx);
                        }
                        glass_toast::say(trf("Created {name}", &[("name", &i.name)]));
                    }
                    Err(e) => this.library.share_note = Some(e),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod log_tests {
    use super::missing_mod_message;
    #[test]
    fn recognises_loader_and_handshake_errors() {
        assert!(missing_mod_message(
            "[Render thread/ERROR] Incompatible mods found!"
        ));
        assert!(missing_mod_message(
            "Mod X requires version >=1 of Y, which is missing!"
        ));
        assert!(!missing_mod_message("[INFO] Loading 20 mods"));
        assert!(!missing_mod_message("Failed to connect: timed out"));
    }
}
