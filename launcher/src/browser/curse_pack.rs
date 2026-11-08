//! Import a CurseForge manifest using only files authorized by its official API.
use super::{
    curseforge::CurseForge,
    installed::{self, Step},
    java_instance::Target,
    Kind, Source,
};
use crate::{
    i18n::{tr, trf},
    instances::{self, Instance},
    loaders::{Kind as LoaderKind, Loader},
};
use serde::Deserialize;
use std::{io::Read, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    name: String,
    manifest_type: String,
    manifest_version: u32,
    minecraft: Game,
    files: Vec<Item>,
    overrides: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Game {
    version: String,
    mod_loaders: Vec<GameLoader>,
}
#[derive(Deserialize)]
struct GameLoader {
    id: String,
    primary: bool,
}
#[derive(Deserialize)]
struct Item {
    #[serde(rename = "projectID")]
    project: u64,
    #[serde(rename = "fileID")]
    file: u64,
    required: bool,
}

fn loader_for(game: &Game) -> Result<Option<Loader>, String> {
    let selected = game
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| game.mod_loaders.first());
    selected
        .map(|l| {
            let (id, version) = l.id.split_once('-').ok_or(tr("Invalid pack manifest."))?;
            Ok(Loader {
                kind: LoaderKind::parse(id)?,
                version: version.into(),
            })
        })
        .transpose()
}

pub fn import(
    path: &Path,
    progress: &dyn Fn(&str, f32),
    expected: Option<&Target>,
) -> Result<Instance, String> {
    let mut archive = crate::import::open(path)?;
    let entry = archive
        .by_name("manifest.json")
        .map_err(|_| tr("Invalid pack manifest."))?;
    if entry.size() > 2 << 20 {
        return Err(tr("Invalid pack manifest.").into());
    }
    let pack: Manifest = serde_json::from_reader(entry.take(2 << 20))
        .map_err(|e| format!("{} {e}", tr("Invalid pack manifest.")))?;
    if pack.manifest_type != "minecraftModpack"
        || pack.manifest_version != 1
        || pack.files.len() > 1500
        || pack.name.trim().is_empty()
    {
        return Err(tr("Invalid pack manifest.").into());
    }
    let loader = loader_for(&pack.minecraft)?;
    let target = Target {
        minecraft: pack.minecraft.version.clone(),
        loader: loader.as_ref().map(|l| l.kind.id().to_owned()),
    };
    if expected.is_some_and(|t| t != &target) {
        return Err(tr("The file does not match this library's game version and loader.").into());
    }
    crate::minecraft_versions::load()?.find(&target.minecraft)?;
    if let Some(l) = &loader {
        crate::loaders::choose(l.kind.id(), &target.minecraft, Some(&l.version))?;
    }
    let source = CurseForge::from_env();
    let mut steps = Vec::new();
    for item in pack.files.into_iter().filter(|f| f.required) {
        let service = source
            .as_ref()
            .ok_or(tr("A CurseForge API key is required for this pack."))?;
        let project = service.project(&item.project.to_string())?;
        progress(&project.title, 0.);
        let mut version = service.file(item.project, item.file)?;
        if project.kind == Kind::ResourcePack && version.loaders.is_empty() {
            version.loaders.push("minecraft".into());
        }
        if let Some(why) = version.blocked {
            return Err(why);
        }
        let folder = target.folder(project.kind)?;
        if target
            .pick(project.kind, std::slice::from_ref(&version))
            .is_none()
        {
            return Err(trf(
                "No compatible file for {name}.",
                &[("name", &project.title)],
            ));
        }
        steps.push((
            folder,
            Step {
                project,
                version,
                dependency: false,
            },
        ));
    }
    let names = instances::list();
    let name = (1..)
        .map(|n| {
            if n == 1 {
                pack.name.clone()
            } else {
                format!("{} ({n})", pack.name)
            }
        })
        .find(|n| !names.iter().any(|i| i.name.eq_ignore_ascii_case(n)))
        .expect("finite instance names");
    let made = instances::create_java(&name, &target.minecraft)?;
    let result = (|| {
        let instance = instances::edit(&made.id, |m| {
            if let Some(j) = &mut m.java {
                j.loader = loader;
            }
        })?;
        crate::import::curse_overrides(
            path,
            &instance.home,
            pack.overrides.as_deref().unwrap_or("overrides"),
            progress,
        )?;
        if let Some(service) = source {
            for (folder, step) in steps {
                installed::put(&service, vec![step], &instance.home, &|_| folder, progress)?;
            }
        }
        Ok(instance)
    })();
    if result.is_err() {
        instances::delete(&made.id)?;
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_primary_and_preserves_loader_version() {
        let game = Game {
            version: "1.20.1".into(),
            mod_loaders: vec![GameLoader {
                id: "forge-47.1.0".into(),
                primary: true,
            }],
        };
        let loader = loader_for(&game).unwrap().unwrap();
        assert_eq!(loader.kind, LoaderKind::Forge);
        assert_eq!(loader.version, "47.1.0");
    }
}
