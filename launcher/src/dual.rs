//! Local installations are discovery inputs. Game binaries and loader metadata used at
//! launch always come from the launcher's usual verified distribution channels.
use crate::{instances, loaders, minecraft_versions};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Official,
    Tlauncher,
    #[default]
    #[serde(other)]
    Auto,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "All local sources",
            Self::Official => "Official launcher",
            Self::Tlauncher => "TLauncher installations",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    root: PathBuf,
    game: PathBuf,
    id: String,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub origin: Origin,
    pub name: String,
    pub minecraft: String,
    pub loader: Option<loaders::Loader>,
    pub supported: bool,
    pub official: bool,
    pub tlauncher: bool,
    pub mods: Arc<Vec<PathBuf>>,
}

impl Entry {
    pub fn matches(&self, source: Source) -> bool {
        match source {
            Source::Auto => true,
            Source::Official => self.official,
            Source::Tlauncher => self.tlauncher,
        }
    }
    pub fn folder(&self) -> &Path {
        &self.origin.game
    }
    pub fn label(&self) -> &'static str {
        match (self.official, self.tlauncher) {
            (true, true) => "Shared Minecraft and TLauncher folder",
            (_, true) => Source::Tlauncher.label(),
            (true, false) => Source::Official.label(),
            (false, false) => "Local Java installation",
        }
    }
}

fn plain(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
}

fn directory(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| plain(&m) && m.is_dir())
}
fn regular(p: &Path) -> bool {
    fs::symlink_metadata(p).is_ok_and(|m| plain(&m) && m.is_file())
}
pub(crate) fn ordinary_file_under(root: &Path, relative: &Path) -> bool {
    if !directory(root) || relative.as_os_str().is_empty() {
        return false;
    }
    let mut path = root.to_owned();
    let mut components = relative.components().peekable();
    while let Some(std::path::Component::Normal(c)) = components.next() {
        path.push(c);
        if components.peek().is_none() {
            return regular(&path);
        }
        if !directory(&path) {
            return false;
        }
    }
    false
}
fn small_json(path: &Path) -> Option<Value> {
    if !regular(path) {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(4_194_305)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 4_194_304)
        .then(|| serde_json::from_slice(&bytes).ok())
        .flatten()
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 160
        && !s.starts_with('.')
        && !s.ends_with([' ', '.'])
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-+ ".contains(&c))
}

fn mod_files(game: &Path) -> Arc<Vec<PathBuf>> {
    let dir = game.join("mods");
    let mut files = Vec::new();
    if directory(&dir) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
                && regular(&path)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Arc::new(files)
}

fn target(id: &str, meta: &Value) -> (String, Option<loaders::Loader>, bool) {
    let parent = meta["inheritsFrom"]
        .as_str()
        .or_else(|| meta["jar"].as_str().filter(|v| *v != id))
        .or_else(|| meta["clientVersion"].as_str().filter(|v| *v != id))
        .or_else(|| {
            meta.pointer("/modpackVersion/gameVersion/name")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            meta["libraries"].as_array()?.iter().find_map(|lib| {
                let coordinate = lib["name"]
                    .as_str()?
                    .strip_prefix("net.minecraftforge:forge:")?;
                let (base, _) = coordinate.split_once('-')?;
                (base.starts_with(|c: char| c.is_ascii_digit()) && safe_id(base)).then_some(base)
            })
        });
    let parent = parent.unwrap_or(id);
    let candidate = meta["libraries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|lib| lib["name"].as_str())
        .find_map(|coord| {
            let (artifact, version) = coord.rsplit_once(':')?;
            let kind = match artifact {
                "net.fabricmc:fabric-loader" => loaders::Kind::Fabric,
                "org.quiltmc:quilt-loader" => loaders::Kind::Quilt,
                "net.neoforged:neoforge" => loaders::Kind::NeoForge,
                "net.minecraftforge:forge" => loaders::Kind::Forge,
                _ => return None,
            };
            let version = version
                .strip_prefix(&format!("{parent}-"))
                .unwrap_or(version);
            Some(loaders::Loader {
                kind,
                version: version.into(),
            })
        })
        .or_else(|| {
            let pair = id
                .strip_prefix("fabric-loader-")
                .map(|v| (loaders::Kind::Fabric, v))
                .or_else(|| {
                    id.strip_prefix("quilt-loader-")
                        .map(|v| (loaders::Kind::Quilt, v))
                })
                .or_else(|| {
                    id.strip_prefix("neoforge-")
                        .map(|v| (loaders::Kind::NeoForge, v))
                })
                .or_else(|| {
                    id.strip_prefix(&format!("{parent}-forge-"))
                        .map(|v| (loaders::Kind::Forge, v))
                })?;
            Some(loaders::Loader {
                kind: pair.0,
                version: pair
                    .1
                    .strip_suffix(&format!("-{parent}"))
                    .unwrap_or(pair.1)
                    .into(),
            })
        })
        .filter(|l| safe_id(&l.version));
    let supported = safe_id(parent) && (candidate.is_some() || parent == id);
    (parent.to_owned(), candidate, supported)
}

/// A root may be shared by both launchers. A modpack's isolated game folder takes
/// precedence over the shared mods directory. No third-party launch arguments are imported.
pub fn inspect(root: &Path) -> Vec<Entry> {
    inspect_as(root, Source::Auto)
}

fn inspect_as(root: &Path, source: Source) -> Vec<Entry> {
    let versions = root.join("versions");
    if !directory(root) || !directory(&versions) {
        return Vec::new();
    }
    let name = root
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let tl = source == Source::Tlauncher
        || name.contains("tlauncher")
        || root
            .ancestors()
            .take(3)
            .filter_map(Path::file_name)
            .any(|n| n.eq_ignore_ascii_case(".tlauncher") || n.eq_ignore_ascii_case("tlauncher"))
        || [
            "TLauncherProfiles.json",
            "tlauncher_profiles.json",
            "tlauncher_libraries.json",
            ".tlauncher",
            "tlauncher_libraries",
        ]
        .iter()
        .any(|s| root.join(s).exists());
    let profile = [
        "launcher_profiles.json",
        "launcher_profiles_microsoft_store.json",
    ]
    .map(|n| root.join(n));
    let name_of_root_is_official = name == ".minecraft";
    let has_profiles = profile.iter().any(|p| regular(p));
    let known = tl || source == Source::Official || name == ".minecraft" || has_profiles;
    let mut profiles = Vec::new();
    for p in profile.iter().filter_map(|p| small_json(p)) {
        for value in p["profiles"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, v)| v)
        {
            if let (Some(id), Some(path)) =
                (value["lastVersionId"].as_str(), value["gameDir"].as_str())
            {
                let path = PathBuf::from(path);
                if path.is_absolute() && directory(&path) {
                    profiles.push((
                        id.to_owned(),
                        value["name"].as_str().unwrap_or(id).to_owned(),
                        path,
                    ));
                }
            }
        }
    }
    let mut result = Vec::new();
    let shared_mods = mod_files(root);
    for dir in fs::read_dir(&versions).into_iter().flatten().flatten() {
        let path = dir.path();
        let id = dir.file_name().to_string_lossy().into_owned();
        if id.is_empty()
            || id.starts_with('.')
            || id
                .chars()
                .any(|c| c.is_control() || r#"/\:*?"<>|"#.contains(c))
            || !directory(&path)
        {
            continue;
        }
        let metadata = small_json(&path.join(format!("{id}.json")));
        let version = metadata.as_ref().unwrap_or(&Value::Null);
        let is_tl =
            tl || !version["modpackVersion"].is_null() || !version["tlauncherVersion"].is_null();
        let manifest_id = version["id"].as_str().unwrap_or(&id);
        if (manifest_id != id && (!is_tl || !safe_id(manifest_id)))
            || (metadata.is_none()
                && !(known
                    && (regular(&path.join(format!("{id}.jar")))
                        || (id.starts_with(|c: char| c.is_ascii_digit()) && safe_id(&id)))))
            || (!known
                && version["mainClass"].is_null()
                && version["downloads"]["client"].is_null()
                && version["inheritsFrom"].is_null())
        {
            continue;
        }
        let (minecraft, loader, supported) = target(manifest_id, version);
        let game = if is_tl && (directory(&path.join("mods")) || directory(&path.join("saves"))) {
            &path
        } else {
            root
        };
        let mut add = |name: String, game: &Path| {
            let origin = Origin {
                root: root.into(),
                game: game.into(),
                id: id.clone(),
            };
            if result.iter().any(|e: &Entry| e.origin == origin) {
                return;
            }
            result.push(Entry {
                origin,
                name,
                minecraft: minecraft.clone(),
                loader: loader.clone(),
                supported,
                official: source == Source::Official
                    || has_profiles
                    || (name_of_root_is_official && !is_tl),
                tlauncher: is_tl,
                mods: if game == root {
                    shared_mods.clone()
                } else {
                    mod_files(game)
                },
            });
        };
        add(id.clone(), game);
        for (_, name, game) in profiles.iter().filter(|(v, _, _)| v == &id) {
            add(name.clone(), game);
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

#[cfg(windows)]
fn volumes() -> Vec<PathBuf> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    // SAFETY: drive roots are NUL-terminated and live through GetDriveTypeW.
    let mask = unsafe { GetLogicalDrives() };
    (0..26)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let root = format!("{}:\\", char::from(b'A' + i));
            let wide = root.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            matches!(unsafe { GetDriveTypeW(wide.as_ptr()) }, 2 | 3 | 6)
                .then(|| PathBuf::from(root))
        })
        .collect()
}
#[cfg(not(windows))]
fn volumes() -> Vec<PathBuf> {
    dirs::home_dir()
        .into_iter()
        .chain(["/Volumes", "/media", "/mnt"].map(PathBuf::from))
        .filter(|d| directory(d))
        .collect()
}

fn ignore(name: &str) -> bool {
    matches!(
        name,
        "windows"
            | "$recycle.bin"
            | "system volume information"
            | "winsxs"
            | ".git"
            | ".rustup"
            | ".cargo"
            | "node_modules"
            | "target"
            | "__pycache__"
            | "windowsapps"
            | "cache"
            | "caches"
            | "temp"
    )
}

mod installed;
pub use installed::InstalledLauncher;

#[derive(Default, Debug)]
pub struct Summary {
    pub installed: Vec<InstalledLauncher>,
    pub volumes: usize,
    pub skipped: usize,
}

/// Inspect quick locations first, then walk mounted volumes in breadth-first order. Emit
/// discoveries immediately. Read directory names once and only open game metadata at candidates.
fn walk(
    roots: Vec<PathBuf>,
    quick: Vec<(PathBuf, Source)>,
    excluded: &[PathBuf],
    cancel: &AtomicBool,
    mut found: impl FnMut(Vec<Entry>),
) -> Summary {
    let mut summary = Summary {
        volumes: roots.len(),
        ..Default::default()
    };
    let mut checked = HashSet::new();
    let mut probe = |dir: &Path, hint: Source, checked: &mut HashSet<PathBuf>| {
        if excluded.iter().any(|p| dir.starts_with(p)) || !checked.insert(dir.to_owned()) {
            return;
        }
        let entries = inspect_as(dir, hint);
        if !entries.is_empty() {
            found(entries);
        }
    };
    for (dir, hint) in quick {
        if cancel.load(Ordering::Relaxed) {
            return summary;
        }
        probe(&dir, hint, &mut checked);
    }
    let mut queue: VecDeque<_> = roots.into();
    while let Some(dir) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if excluded.iter().any(|p| dir.starts_with(p)) || !directory(&dir) {
            continue;
        }
        let Ok(iter) = fs::read_dir(&dir) else {
            summary.skipped += 1;
            continue;
        };
        for entry in iter.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return summary;
            }
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name == "versions" {
                probe(&dir, Source::Auto, &mut checked);
            } else if !ignore(&name)
                && !(checked.contains(&dir)
                    && matches!(
                        name.as_str(),
                        "assets" | "libraries" | "saves" | "mods" | "runtime"
                    ))
            {
                queue.push_back(entry.path());
            }
        }
    }
    summary
}

pub fn discover(
    manual: Option<PathBuf>,
    cancel: &AtomicBool,
    found: impl FnMut(Vec<Entry>),
) -> Summary {
    discover_for(manual, Source::Auto, true, cancel, found)
}

fn selected_root(path: &Path) -> &Path {
    let versions = if path
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("versions"))
    {
        Some(path)
    } else {
        path.parent().filter(|p| {
            p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("versions"))
        })
    };
    versions.and_then(Path::parent).unwrap_or(path)
}

fn quick_locations(base: &Path) -> Vec<(PathBuf, Source)> {
    let tl = base.join(".tlauncher");
    let default = base.join(".minecraft");
    let mut paths = Vec::new();
    let mut configured = None;
    // Read only the game directory setting. Do not use account files or launcher arguments.
    for config in [
        tl.join("tlauncher-2.0.properties"),
        default.join("tlauncher-2.0.properties"),
    ] {
        if !regular(&config) {
            continue;
        }
        let mut text = String::new();
        if fs::File::open(config)
            .and_then(|f| f.take(512 * 1024).read_to_string(&mut text))
            .is_err()
        {
            continue;
        }
        if let Some(value) = text
            .lines()
            .filter_map(|l| l.trim().split_once('='))
            .find_map(|(k, v)| (k.trim() == "minecraft.gamedir").then(|| v.trim()))
        {
            let path = PathBuf::from(value.replace(r"\:", ":").replace(r"\\", r"\"));
            if path.is_absolute() {
                if directory(&path) {
                    paths.push((path.clone(), Source::Tlauncher));
                }
                configured = Some(path);
                break;
            }
        }
    }
    let shared = configured.as_ref().is_none_or(|path| *path == default)
        && (directory(&tl) || regular(&default.join("tlauncher-2.0.properties")));
    paths.push((
        default,
        if shared {
            Source::Tlauncher
        } else {
            Source::Auto
        },
    ));
    paths.push((tl, Source::Tlauncher));
    paths
}

pub fn discover_for(
    manual: Option<PathBuf>,
    source: Source,
    all_drives: bool,
    cancel: &AtomicBool,
    mut found: impl FnMut(Vec<Entry>),
) -> Summary {
    if let Some(path) = manual {
        if !cancel.load(Ordering::Relaxed) {
            let rows = inspect_as(selected_root(&path), source);
            if !rows.is_empty() {
                found(rows);
            }
        }
        return Summary::default();
    }
    let roots = if all_drives { volumes() } else { Vec::new() };
    let quick = dirs::data_dir()
        .into_iter()
        .flat_map(|base| quick_locations(&base))
        .chain(
            dirs::home_dir()
                .into_iter()
                .flat_map(|base| quick_locations(&base)),
        )
        .collect();
    walk(
        roots,
        quick,
        &[crate::backend::data_dir(), crate::backend::game_home()],
        cancel,
        found,
    )
}

const ORIGIN: &str = ".bodka-import.json";

#[derive(Serialize, Deserialize)]
struct ModFile {
    name: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
struct Receipt {
    #[serde(flatten)]
    origin: Origin,
    #[serde(default)]
    mods: Vec<ModFile>,
}

pub(crate) fn source_for(instance: &instances::Instance) -> Option<PathBuf> {
    let origin: Origin = serde_json::from_value(small_json(&instance.home.join(ORIGIN))?).ok()?;
    Some(origin.root)
}

pub fn imported() -> Vec<(Origin, String)> {
    instances::list()
        .into_iter()
        .filter(|i| i.java.is_some())
        .filter_map(|i| {
            Some((
                serde_json::from_value(small_json(&i.home.join(ORIGIN))?).ok()?,
                i.id,
            ))
        })
        .collect()
}

/// Import only the mod files displayed in the catalog. Repeated actions use the same
/// managed instance. A failed import rolls back the new instance, never the source.
pub fn import(entry: &Entry) -> Result<instances::Instance, String> {
    if let Some((_, id)) = imported().into_iter().find(|(key, _)| key == &entry.origin) {
        return instances::find(&id);
    }
    if !entry.supported {
        return Err(crate::i18n::tr("Unsupported local profile").into());
    }
    let catalog = minecraft_versions::load()?;
    let version = catalog.find(&entry.minecraft).map_err(|_| crate::i18n::trf(
        "Mojang does not list {version}. Open this profile in its original launcher or download an official version with BODKA.",
        &[("version", &entry.minecraft)],
    ))?;
    let loader = entry
        .loader
        .as_ref()
        .map(|l| loaders::choose(l.kind.id(), &version.id, Some(&l.version)))
        .transpose()?;
    let existing: BTreeSet<_> = instances::list()
        .into_iter()
        .map(|i| i.name.to_lowercase())
        .collect();
    let base = format!("BODKA · {}", entry.name);
    let name = (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base} ({n})")
            }
        })
        .find(|n| !existing.contains(&n.to_lowercase()))
        .unwrap();
    let instance = instances::create_java(&name, &version.id)?;
    let finish = || -> Result<instances::Instance, String> {
        let mods = copy_mods(entry, &instance.home)?;
        let instance = instances::edit(&instance.id, |meta| {
            if let Some(java) = meta.java.as_mut() {
                java.loader = loader;
            }
        })?;
        crate::sources::write_file(
            &instance.home.join(ORIGIN),
            &serde_json::to_vec(&Receipt {
                origin: entry.origin.clone(),
                mods,
            })
            .map_err(|e| e.to_string())?,
        )?;
        Ok(instance)
    };
    finish().inspect_err(|_| {
        let _ = instances::delete(&instance.id);
    })
}

fn copy_mods(entry: &Entry, dest: &Path) -> Result<Vec<ModFile>, String> {
    let source = entry.folder().join("mods");
    let mut manifest = Vec::new();
    for file in entry.mods.iter() {
        if file.parent() != Some(source.as_path())
            || !ordinary_file_under(
                entry.folder(),
                &Path::new("mods").join(file.file_name().unwrap()),
            )
        {
            return Err(crate::i18n::trf(
                "Local mod unavailable or changed: {file}",
                &[("file", &file.display().to_string())],
            ));
        }
        let mods = dest.join("mods");
        fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
        let name = file
            .file_name()
            .unwrap()
            .to_str()
            .ok_or_else(|| file.display().to_string())?;
        let path = mods.join(name);
        fs::copy(file, &path).map_err(|e| e.to_string())?;
        manifest.push(ModFile {
            name: name.into(),
            sha256: crate::backend::file_sha256(&path)?,
        });
    }
    Ok(manifest)
}

pub(crate) fn repair_mods(home: &Path) -> Result<(), String> {
    let marker = home.join(ORIGIN);
    if !marker.exists() {
        return Ok(());
    }
    let receipt: Receipt = small_json(&marker)
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| crate::i18n::tr("Unreadable import record").to_string())?;
    for item in receipt.mods {
        let name = Path::new(&item.name);
        let invalid = || {
            crate::i18n::trf(
                "Local mod unavailable or changed: {file}",
                &[("file", &item.name)],
            )
        };
        if name.file_name().and_then(|n| n.to_str()) != Some(&item.name)
            || name
                .extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("jar"))
        {
            return Err(invalid());
        }
        let mods = home.join("mods");
        fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
        if !directory(&mods) {
            return Err(invalid());
        }
        let dest = mods.join(name);
        if regular(&dest) {
            continue;
        }
        let relative = Path::new("mods").join(name);
        if !ordinary_file_under(&receipt.origin.game, &relative) {
            return Err(invalid());
        }
        let temp = tempfile::NamedTempFile::new_in(&mods).map_err(|e| e.to_string())?;
        fs::copy(receipt.origin.game.join(relative), temp.path()).map_err(|e| e.to_string())?;
        if crate::backend::file_sha256(temp.path())? != item.sha256 {
            return Err(invalid());
        }
        if let Err(e) = temp.persist_noclobber(&dest) {
            if e.error.kind() != std::io::ErrorKind::AlreadyExists || !regular(&dest) {
                return Err(e.error.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
