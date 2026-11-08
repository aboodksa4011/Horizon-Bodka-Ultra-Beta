//! A local author's jar without a catalog entry can still declare its loader and supported Minecraft range.
use super::java_instance::Target;
use crate::i18n::{tr, trf};
use semver::{Version, VersionReq};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
};

fn version(s: &str) -> Option<Version> {
    Version::parse(s).ok().or_else(|| {
        if !s.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
            return None;
        }
        match s.split('.').count() {
            1 => Version::parse(&format!("{s}.0.0")).ok(),
            2 => Version::parse(&format!("{s}.0")).ok(),
            _ => None,
        }
    })
}
fn matches_rule(rule: &str, actual: &str) -> bool {
    let rule = rule.trim();
    if rule == "*" || rule == actual {
        return true;
    }
    let Some(v) = version(actual) else {
        return false;
    };
    if rule.len() > 1
        && (rule.starts_with('[') || rule.starts_with('('))
        && (rule.ends_with(']') || rule.ends_with(')'))
    {
        let inside = &rule[1..rule.len() - 1];
        let Some((lo, hi)) = inside.split_once(',') else {
            return rule.starts_with('[')
                && rule.ends_with(']')
                && version(inside).as_ref() == Some(&v);
        };
        return (lo.trim().is_empty()
            || version(lo.trim()).is_some_and(|l| v > l || (v == l && rule.starts_with('['))))
            && (hi.trim().is_empty()
                || version(hi.trim()).is_some_and(|h| v < h || (v == h && rule.ends_with(']'))));
    }
    if rule.split_whitespace().count() > 1 {
        return rule.split_whitespace().all(|s| matches_rule(s, actual));
    }
    if let Some(exact) = version(rule) {
        return exact == v;
    }
    if rule.starts_with(['<', '>', '^', '~', '=']) || rule.ends_with(".x") || rule.ends_with(".*") {
        return VersionReq::parse(&rule.replace(".x", ".*")).is_ok_and(|r| r.matches(&v));
    }
    false
}

fn metadata<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, path: &str) -> Option<String> {
    let entry = zip.by_name(path).ok()?;
    if entry.size() > 1 << 20 {
        return None;
    }
    let mut value = String::new();
    entry.take((1 << 20) + 1).read_to_string(&mut value).ok()?;
    (value.len() <= 1 << 20).then_some(value)
}

fn inspect(path: &Path, t: &Target) -> Result<String, String> {
    let mismatch = tr("The file does not match this library's game version and loader.");
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|_| mismatch)?;
    let mut dependencies = Vec::<String>::new();
    let name = if t.accepts("fabric") && zip.index_for_name("fabric.mod.json").is_some() {
        let raw = metadata(&mut zip, "fabric.mod.json").ok_or(mismatch)?;
        let json: Value = serde_json::from_str(&raw).map_err(|_| mismatch)?;
        if json["environment"] == "server" {
            return Err(mismatch.into());
        }
        let d = json["depends"].as_object().ok_or(mismatch)?;
        let mc = d.get("minecraft").ok_or(mismatch)?;
        let fits = mc.as_str().is_some_and(|r| matches_rule(r, &t.minecraft))
            || mc.as_array().is_some_and(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .any(|r| matches_rule(r, &t.minecraft))
            });
        if !fits {
            return Err(mismatch.into());
        }
        dependencies.extend(
            d.keys()
                .filter(|k| !matches!(k.as_str(), "minecraft" | "fabricloader" | "java"))
                .cloned(),
        );
        json["name"]
            .as_str()
            .or(json["id"].as_str())
            .ok_or(mismatch)?
            .to_string()
    } else {
        let file = match t.loader.as_deref() {
            Some("neoforge") => "META-INF/neoforge.mods.toml",
            Some("forge") => "META-INF/mods.toml",
            _ => return Err(mismatch.into()),
        };
        let raw = metadata(&mut zip, file).ok_or(mismatch)?;
        let doc: toml::Value = toml::from_str(&raw).map_err(|_| mismatch)?;
        let mods = doc
            .get("mods")
            .and_then(toml::Value::as_array)
            .ok_or(mismatch)?;
        let deps = doc
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .ok_or(mismatch)?;
        let mut mc = false;
        for row in deps.values().filter_map(toml::Value::as_array).flatten() {
            let id = row
                .get("modId")
                .and_then(toml::Value::as_str)
                .ok_or(mismatch)?;
            if row.get("side").and_then(toml::Value::as_str) == Some("SERVER") {
                continue;
            }
            if row.get("mandatory").and_then(toml::Value::as_bool) == Some(false)
                || row
                    .get("type")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|s| s != "required")
            {
                continue;
            }
            if id == "minecraft" {
                if !row
                    .get("versionRange")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|r| matches_rule(r, &t.minecraft))
                {
                    return Err(mismatch.into());
                }
                mc = true;
            } else if !matches!(id, "forge" | "neoforge" | "java") {
                dependencies.push(id.into());
            }
        }
        if !mc {
            return Err(mismatch.into());
        }
        let first = mods.first().ok_or(mismatch)?;
        first
            .get("displayName")
            .or_else(|| first.get("modId"))
            .and_then(toml::Value::as_str)
            .ok_or(mismatch)?
            .to_string()
    };
    if !dependencies.is_empty() {
        return Err(trf(
            "This local mod requires a published dependency list: {names}",
            &[("names", &dependencies.join(", "))],
        ));
    }
    Ok(name)
}
/// Inspect the staged bytes, then publish with no overwrite, even on simultaneous drops.
pub(super) fn add(file: &Path, target: &Target, home: &Path) -> Result<String, String> {
    let _lock = super::lock_home(home);
    let name = file
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(super::safe_name)
        .filter(|s| s.to_ascii_lowercase().ends_with(".jar"))
        .ok_or(tr("Invalid pack manifest."))?;
    let dir = home.join("mods");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    let mut src = fs::File::open(file)
        .map_err(|e| e.to_string())?
        .take((512 << 20) + 1);
    if std::io::copy(&mut src, temp.as_file_mut()).map_err(|e| e.to_string())? > 512 << 20 {
        return Err(tr("Invalid pack manifest.").into());
    }
    let title = inspect(temp.path(), target)?;
    if dir.join(format!("{name}.disabled")).exists() {
        return Err(tr("A file with this name already exists.").into());
    }
    temp.persist_noclobber(dir.join(name))
        .map_err(|e| e.error.to_string())?;
    Ok(title)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_fabric_and_maven_ranges_conservatively() {
        assert!(matches_rule(">=1.20 <1.21", "1.20.4"));
        assert!(matches_rule("[1.20,1.21)", "1.20.4"));
        assert!(!matches_rule("[1.20,1.21)", "1.21"));
        assert!(!matches_rule("1.20.1", "1.20.2"));
        assert!(matches_rule("1.20.x", "1.20.6"));
        assert!(!matches_rule("unknown", "1.20.1"));
    }
}
