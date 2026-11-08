//! A portable library code contains registry identities, never URLs, command lines, paths or credentials.
use super::{
    installed::{self, Installed, Step},
    java_instance, Kind, SourceId,
};
use crate::{
    i18n::{tr, trf},
    instances::{self, Instance},
    loaders::Loader,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const PREFIX: &str = "BODKA1.";
const MAX_BYTES: usize = 192 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<Loader>,
    pub horizon: bool,
    pub projects: Vec<Pin>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    source: SourceId,
    project: String,
    version: String,
    enabled: bool,
    dependency: bool,
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b))
        && !id.starts_with('.')
        && !id.ends_with('.')
        && !id.contains("..")
}

fn matches_checksum(path: &std::path::Path, wanted: &str) -> bool {
    use sha2::Digest;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    macro_rules! verify {
        ($kind:ty,$hex:expr) => {{
            let mut h = <$kind>::new();
            std::io::copy(&mut f, &mut h).is_ok()
                && format!("{:x}", h.finalize()).eq_ignore_ascii_case($hex)
        }};
    }
    match wanted.split_once(':') {
        Some(("SHA-512", hex)) => verify!(sha2::Sha512, hex),
        Some(("SHA-256", hex)) => verify!(sha2::Sha256, hex),
        Some(("SHA-1", hex)) => verify!(sha1::Sha1, hex),
        _ => false,
    }
}

impl Profile {
    pub fn of(instance: &Instance, horizon_tag: Option<&str>) -> Result<Self, String> {
        let entries = Installed::load(&instance.home).entries;
        let (minecraft, loader, horizon) = match &instance.java {
            Some(game) => (game.minecraft.clone(), game.loader.clone(), false),
            None => (
                horizon_tag
                    .ok_or(tr("Choose an installed Horizon version first."))?
                    .to_string(),
                None,
                true,
            ),
        };
        let mut projects = Vec::new();
        for e in &entries {
            if horizon
                || !matches!(e.source, SourceId::Modrinth | SourceId::CurseForge)
                || !matches!(e.kind, Kind::Mod | Kind::ResourcePack | Kind::Shader)
            {
                return Err(tr("Use file export for this library's local content.").into());
            }
            projects.push(Pin {
                source: e.source,
                project: e.project.clone(),
                version: e.version.clone(),
                dependency: e.dependency,
                enabled: java_instance::is_on(&instance.home, e),
            });
        }
        for dir in ["mods", "horizon-mods", "resourcepacks", "shaderpacks"] {
            let folder = instance.home.join(dir);
            if !folder.exists() {
                continue;
            }
            for item in std::fs::read_dir(folder).map_err(|e| e.to_string())? {
                let item = item.map_err(|e| e.to_string())?;
                let relative = format!("{dir}/{}", item.file_name().to_string_lossy());
                let file = relative.strip_suffix(".disabled").unwrap_or(&relative);
                if !entries.iter().any(|e| e.file == file) {
                    return Err(tr("Use file export for this library's local content.").into());
                }
            }
        }
        let profile = Self {
            name: instance.name.clone(),
            minecraft,
            loader,
            horizon,
            projects,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn code(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err(tr("The library code is too large.").into());
        }
        Ok(format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes)))
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        let invalid = tr("Invalid BODKA library code.");
        if input.len() > MAX_BYTES * 2 {
            return Err(invalid.into());
        }
        let data = URL_SAFE_NO_PAD
            .decode(input.trim().strip_prefix(PREFIX).ok_or(invalid)?)
            .map_err(|_| invalid)?;
        if data.len() > MAX_BYTES {
            return Err(invalid.into());
        }
        let profile: Self = serde_json::from_slice(&data).map_err(|_| invalid)?;
        profile.validate()?;
        Ok(profile)
    }

    fn validate(&self) -> Result<(), String> {
        let mut seen = HashSet::new();
        if self.name.trim().is_empty()
            || self.name.chars().count() > 100
            || self.name.chars().any(char::is_control)
            || !identifier(&self.minecraft)
            || self
                .loader
                .as_ref()
                .is_some_and(|l| !identifier(&l.version))
            || self.projects.len() > 512
            || (self.horizon && (self.loader.is_some() || !self.projects.is_empty()))
            || self.projects.iter().any(|p| {
                !matches!(p.source, SourceId::Modrinth | SourceId::CurseForge)
                    || !identifier(&p.project)
                    || !identifier(&p.version)
                    || !seen.insert((p.source, p.project.as_str()))
            })
        {
            return Err(tr("Invalid BODKA library code.").into());
        }
        Ok(())
    }

    /// Compare the pinned versions, enabled state and actual files in the recipient's library.
    pub fn missing(&self, instance: &Instance) -> usize {
        let manifest = Installed::load(&instance.home);
        self.projects
            .iter()
            .filter(|p| {
                !manifest.find(p.source, &p.project).is_some_and(|e| {
                    let path = if p.enabled {
                        instance.home.join(&e.file)
                    } else {
                        instance.home.join(format!("{}.disabled", e.file))
                    };
                    e.version == p.version
                        && matches_checksum(&path, &e.checksum)
                        && java_instance::is_on(&instance.home, e) == p.enabled
                })
            })
            .count()
    }

    /// Build a separate library. Validate pins and their required dependencies before creating it.
    pub fn import(&self, progress: &dyn Fn(&str, f32)) -> Result<Instance, String> {
        self.validate()?;
        let mut steps = Vec::new();
        let target = java_instance::Target {
            minecraft: self.minecraft.clone(),
            loader: self.loader.as_ref().map(|l| l.kind.id().into()),
        };
        if !self.horizon {
            crate::minecraft_versions::load()?.find(&self.minecraft)?;
            if let Some(l) = &self.loader {
                crate::loaders::choose(l.kind.id(), &self.minecraft, Some(&l.version))?;
            }
        }
        for pin in &self.projects {
            let src = super::source(pin.source)
                .ok_or(tr("The library's content source is unavailable."))?;
            progress(&pin.project, 0.);
            let project = src.project(&pin.project)?;
            if project.id != pin.project {
                return Err(tr("Invalid BODKA library code.").into());
            }
            target.folder(project.kind)?;
            let versions = src.versions_for(&project, &target)?;
            let version = versions
                .into_iter()
                .find(|v| v.id == pin.version)
                .filter(|v| {
                    target.pick(project.kind, std::slice::from_ref(v)).is_some()
                        && v.file().is_some_and(|f| f.checksum().is_some())
                })
                .ok_or_else(|| {
                    trf(
                        "No compatible file for {name}.",
                        &[("name", &project.title)],
                    )
                })?;
            if version.required.iter().any(|id| {
                !self
                    .projects
                    .iter()
                    .any(|p| p.source == pin.source && p.project == *id)
            }) {
                return Err(tr("The code is missing a required dependency.").into());
            }
            steps.push((
                pin,
                Step {
                    project,
                    version,
                    dependency: pin.dependency,
                },
            ));
        }
        let existing = instances::list();
        let name = (1..)
            .map(|i| {
                if i == 1 {
                    self.name.clone()
                } else {
                    format!("{} ({i})", self.name)
                }
            })
            .find(|n| !existing.iter().any(|i| i.name.eq_ignore_ascii_case(n)))
            .expect("finite names");
        let made = if self.horizon {
            instances::create(&name, None)?
        } else {
            instances::create_java(&name, &self.minecraft)?
        };
        let result = (|| {
            let instance = instances::edit(&made.id, |m| {
                if self.horizon {
                    m.version = Some(self.minecraft.clone());
                } else if let Some(j) = m.java.as_mut() {
                    j.loader = self.loader.clone();
                }
            })?;
            for (pin, step) in steps {
                let src = super::source(pin.source)
                    .ok_or(tr("The library's content source is unavailable."))?;
                let folder = target.folder(step.project.kind)?;
                let rows =
                    installed::put(&*src, vec![step], &instance.home, &|_| folder, progress)?;
                if !pin.enabled {
                    for e in rows {
                        java_instance::set_on(&instance.home, &e, false)?;
                    }
                }
            }
            Ok(instance)
        })();
        if result.is_err() {
            instances::delete(&made.id)?;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_round_trip_and_reject_paths_or_duplicate_pins() {
        let mut p = Profile {
            name: "مكتبتي".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            horizon: false,
            projects: vec![],
        };
        assert_eq!(Profile::parse(&p.code().unwrap()).unwrap(), p);
        assert!(Profile::parse("https://example.test/pack").is_err());
        let pin = Pin {
            source: SourceId::Modrinth,
            project: "abc".into(),
            version: "def".into(),
            enabled: true,
            dependency: false,
        };
        p.projects = vec![pin.clone(), pin];
        assert!(Profile::parse(&p.code().unwrap()).is_err());
        p.projects.clear();
        p.minecraft = "../elsewhere".into();
        assert!(Profile::parse(&p.code().unwrap()).is_err());
    }
}
