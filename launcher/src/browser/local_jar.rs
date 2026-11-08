//! Match a dropped file by its bytes; use the published version and the regular dependency installer.
use super::{
    download,
    java_instance::{self, Target},
    modrinth::{self, Modrinth},
    FileRef, Kind, Project, Query, Source, SourceId, Version,
};
use crate::i18n::tr;
use sha2::{Digest, Sha512};
use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

struct LocalFile<'a> {
    path: &'a Path,
    project: String,
    version: Version,
    hash: String,
}
impl Source for LocalFile<'_> {
    fn id(&self) -> SourceId {
        SourceId::Modrinth
    }
    fn search(&self, q: &Query) -> Result<Vec<Project>, String> {
        Modrinth.search(q)
    }
    fn project(&self, id: &str) -> Result<Project, String> {
        Modrinth.project(id)
    }
    fn versions(&self, p: &Project) -> Result<Vec<Version>, String> {
        if p.id == self.project {
            Ok(vec![self.version.clone()])
        } else {
            Modrinth.versions(p)
        }
    }
    fn versions_for(&self, p: &Project, t: &Target) -> Result<Vec<Version>, String> {
        if p.id == self.project {
            self.versions(p)
        } else {
            Modrinth.versions_for(p, t)
        }
    }
    fn fetch(&self, f: &FileRef, dir: &Path, progress: &dyn Fn(f32)) -> Result<PathBuf, String> {
        if f.sha512.as_deref() == Some(&self.hash) {
            download::save(
                f,
                File::open(self.path).map_err(|e| e.to_string())?,
                dir,
                progress,
            )
        } else {
            Modrinth.fetch(f, dir, progress)
        }
    }
}
pub fn install(path: &Path, target: &Target, home: &Path) -> Result<Vec<String>, String> {
    target.folder(Kind::Mod)?;
    let mut reader = File::open(path).map_err(|e| e.to_string())?;
    let meta = reader.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 512 << 20 {
        return Err(tr("This mod file could not be matched to a published version.").into());
    }
    let mut digest = Sha512::new();
    io::copy(&mut reader, &mut digest).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", digest.finalize());
    let info = match modrinth::get(&format!(
        "https://api.modrinth.com/v2/version_file/{hash}?algorithm=sha512"
    )) {
        Ok(info) => info,
        Err(e) if e.contains("404") || e.starts_with("No connection") => {
            return super::local_metadata::add(path, target, home).map(|name| vec![name])
        }
        Err(e) => return Err(e),
    };
    let id = info["project_id"].as_str().ok_or(tr(
        "This mod file could not be matched to a published version.",
    ))?;
    let mut version = modrinth::parse_version(&info).ok_or(tr(
        "The file does not match this library's game version and loader.",
    ))?;
    version.files.retain(|f| {
        f.sha512.as_deref() == Some(&hash) && f.name.to_ascii_lowercase().ends_with(".jar")
    });
    let project = Modrinth.project(id)?;
    if project.kind != Kind::Mod
        || target
            .pick(Kind::Mod, std::slice::from_ref(&version))
            .is_none()
    {
        return Err(tr("The file does not match this library's game version and loader.").into());
    }
    let source = LocalFile {
        path,
        project: id.into(),
        version,
        hash,
    };
    java_instance::install(&source, target, &project, home, &|_, _| {})
        .map(|entries| entries.into_iter().map(|e| e.title).collect())
}
