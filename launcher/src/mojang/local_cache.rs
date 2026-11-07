//! Read-only reuse of local Minecraft files requested by the owner-checked downloader.
use super::{relative, sha1_hex, OBJECTS};
use crate::minecraft_versions::Version;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) struct LocalFiles {
    root: PathBuf,
    artifacts: HashMap<String, (PathBuf, String)>,
}

fn checked(root: &Path, file: &Path, sha1: &str, limit: u64) -> Option<Vec<u8>> {
    if !crate::dual::ordinary_file_under(root, file) {
        return None;
    }
    let input = fs::File::open(root.join(file)).ok()?;
    if input.metadata().ok()?.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    input.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit && sha1_hex(&bytes) == sha1).then_some(bytes)
}

impl LocalFiles {
    pub(crate) fn open(root: &Path, version: &Version) -> Self {
        let mut source = Self {
            root: root.to_owned(),
            artifacts: HashMap::new(),
        };
        let id = &version.id;
        let Some(dir) = relative(id)
            .ok()
            .filter(|p| p.components().count() == 1)
            .map(|p| Path::new("versions").join(p))
        else {
            return source;
        };
        let manifest = dir.join(format!("{id}.json"));
        let Some(body) = checked(root, &manifest, &version.sha1, 4 * 1024 * 1024) else {
            return source;
        };
        let Ok(meta) = serde_json::from_slice::<Value>(&body) else {
            return source;
        };
        source
            .artifacts
            .insert(version.url.clone(), (manifest, version.sha1.clone()));
        let mut add = |item: &Value, path: PathBuf| {
            if let (Some(url), Some(sha1)) = (item["url"].as_str(), item["sha1"].as_str()) {
                source.artifacts.insert(url.into(), (path, sha1.into()));
            }
        };
        add(&meta["downloads"]["client"], dir.join(format!("{id}.jar")));
        for lib in meta["libraries"].as_array().into_iter().flatten() {
            let downloads = &lib["downloads"];
            for item in std::iter::once(&downloads["artifact"]).chain(
                downloads["classifiers"]
                    .as_object()
                    .into_iter()
                    .flat_map(|m| m.values()),
            ) {
                if let Some(path) = item["path"].as_str().and_then(|p| relative(p).ok()) {
                    add(item, PathBuf::from("libraries").join(path));
                }
            }
        }
        let index = &meta["assetIndex"];
        if let Some(path) = index["id"]
            .as_str()
            .and_then(|id| relative(&format!("{id}.json")).ok())
        {
            add(index, Path::new("assets/indexes").join(path));
        }
        let logging = &meta["logging"]["client"]["file"];
        if let Some(path) = logging["id"].as_str().and_then(|id| relative(id).ok()) {
            add(logging, Path::new("assets/log_configs").join(path));
        }
        source
    }
    pub(crate) fn bytes(&self, url: &str) -> Option<Vec<u8>> {
        if let Some((file, sha1)) = self.artifacts.get(url) {
            return checked(&self.root, file, sha1, 256 * 1024 * 1024);
        }
        let object = url.strip_prefix(OBJECTS)?.strip_prefix('/')?;
        let (prefix, hash) = object.split_once('/')?;
        if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) || prefix != &hash[..2]
        {
            return None;
        }
        checked(
            &self.root,
            &Path::new("assets/objects").join(object),
            hash,
            256 * 1024 * 1024,
        )
    }
}
