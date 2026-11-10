use super::*;
use serde_json::json;

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "bodka-dual-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn version(root: &Path, id: &str, metadata: Value) {
    let dir = root.join("versions").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("{id}.json")), metadata.to_string()).unwrap();
}
fn mods(root: &Path, name: &str) {
    fs::create_dir_all(root.join("mods")).unwrap();
    fs::write(root.join("mods").join(name), b"local fixture").unwrap();
}

#[test]
fn multiple_volumes_stream_shared_roots_once_and_keep_modpacks_separate() {
    let t = Scratch::new("drives");
    let first = t.0.join("first");
    let second = t.0.join("second");
    let shared = first.join("Games/.minecraft");
    let other = second.join("portable/tlauncher/game");
    version(&shared, "1.21.1", json!({"id":"1.21.1"}));
    fs::write(shared.join("TLauncherProfiles.json"), b"{}").unwrap();
    fs::write(shared.join("launcher_profiles.json"), b"{}").unwrap();
    mods(&shared, "one.jar");
    mods(&shared, "readme.txt");
    let pack = "حزمة تجريبية";
    version(
        &other,
        pack,
        json!({"id":pack,"inheritsFrom":"1.21.1","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9"}]}),
    );
    mods(&other, "ignored-global.jar");
    mods(&other.join("versions").join(pack), "my-mod.jar");
    let mut rows = Vec::new();
    let summary = walk(
        vec![first, second],
        vec![(shared.clone(), Source::Auto)],
        &[],
        &AtomicBool::new(false),
        |v| rows.extend(v),
    );
    assert_eq!(summary.volumes, 2);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].matches(Source::Official) && rows[0].matches(Source::Tlauncher));
    assert_eq!(rows[0].mods.as_slice(), [shared.join("mods/one.jar")]);
    let tl = &rows[1];
    assert!(!tl.matches(Source::Official) && tl.matches(Source::Tlauncher) && tl.supported);
    assert_eq!(tl.minecraft, "1.21.1");
    assert_eq!(tl.loader.as_ref().unwrap().version, "0.16.9");
    assert_eq!(
        tl.mods.as_slice(),
        [other.join("versions").join(pack).join("mods/my-mod.jar")]
    );
}
#[test]
fn missing_manifests_and_custom_game_directories_are_visible() {
    let t = Scratch::new("partial");
    let root = t.0.join(".minecraft");
    let profile = t.0.join("other");
    mods(&profile, "example.jar");
    fs::create_dir_all(root.join("versions/1.20.1")).unwrap();
    fs::write(root.join("launcher_profiles.json"),json!({"profiles":{"custom":{"name":"My profile","lastVersionId":"1.20.1","gameDir":profile}}}).to_string()).unwrap();
    let rows = inspect(&root);
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|e| e.folder() == profile && e.mods.len() == 1));
    assert!(inspect(&root.join("absent")).is_empty());
}
#[test]
fn import_copies_the_listed_mods_and_leaves_worlds_and_accounts_in_place() {
    let t = Scratch::new("copy");
    let game = t.0.join(".minecraft");
    version(&game, "1.21", json!({"id":"1.21"}));
    mods(&game, "one.jar");
    fs::create_dir(game.join("saves")).unwrap();
    fs::write(game.join("saves/world"), b"preserve").unwrap();
    fs::write(game.join("accounts.json"), b"private").unwrap();
    let e = inspect(&game).pop().unwrap();
    let dest = t.0.join("managed");
    copy_mods(&e, &dest).unwrap();
    assert_eq!(
        fs::read(dest.join("mods/one.jar")).unwrap(),
        b"local fixture"
    );
    assert!(!dest.join("saves").exists() && !dest.join("accounts.json").exists());
    assert_eq!(fs::read(game.join("saves/world")).unwrap(), b"preserve");
    fs::remove_file(game.join("mods/one.jar")).unwrap();
    assert!(copy_mods(&e, &t.0.join("second")).is_err());
    assert!(!ordinary_file_under(&game, Path::new("../other")));
}
#[test]
fn excludes_managed_trees_and_cancels_discovery() {
    let t = Scratch::new("cancel");
    let root = t.0.join(".minecraft");
    version(&root, "1.20", json!({"id":"1.20"}));
    for (stop, exclusions) in [(true, vec![]), (false, vec![root.clone()])] {
        walk(
            vec![t.0.clone()],
            vec![(root.clone(), Source::Auto)],
            &exclusions,
            &AtomicBool::new(stop),
            |_| panic!("unexpected match"),
        );
    }
    assert!(!target("unknown", &json!({"inheritsFrom":"1.20"})).2);
    assert!(!target("x",&json!({"inheritsFrom":"../bad","libraries":[{"name":"net.fabricmc:fabric-loader:0.16.9"}]})).2);
}

#[test]
fn repair_restores_only_missing_mods_whose_fingerprint_matches() {
    let t = Scratch::new("mod-repair");
    let source = t.0.join(".minecraft");
    version(&source, "1.21", json!({"id":"1.21"}));
    mods(&source, "pack.jar");
    let e = inspect(&source).pop().unwrap();
    let managed = t.0.join("managed");
    let files = copy_mods(&e, &managed).unwrap();
    fs::write(
        managed.join(ORIGIN),
        serde_json::to_vec(&Receipt {
            origin: e.origin,
            mods: files,
        })
        .unwrap(),
    )
    .unwrap();
    let target = managed.join("mods/pack.jar");
    fs::remove_file(&target).unwrap();
    repair_mods(&managed).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"local fixture");
    fs::write(&target, b"player edit").unwrap();
    repair_mods(&managed).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"player edit");
    fs::remove_file(&target).unwrap();
    fs::write(source.join("mods/pack.jar"), b"changed original").unwrap();
    assert!(repair_mods(&managed).is_err());
    assert!(!target.exists());
}

#[test]
fn quick_scan_uses_the_configured_tlauncher_game_directory() {
    let t = Scratch::new("configured-tl");
    let config = t.0.join(".tlauncher");
    let custom = t.0.join("custom games");
    fs::create_dir_all(&config).unwrap();
    version(&custom, "1.20.1", json!({"id":"1.20.1"}));
    let escaped = custom
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace(':', "\\:");
    fs::write(
        config.join("tlauncher-2.0.properties"),
        format!("minecraft.gamedir={escaped}\n"),
    )
    .unwrap();
    version(&t.0.join(".minecraft"), "1.21", json!({"id":"1.21"}));
    let mut entries = Vec::new();
    let summary = walk(
        vec![],
        quick_locations(&t.0),
        &[],
        &AtomicBool::new(false),
        |v| entries.extend(v),
    );
    assert_eq!(summary.volumes, 0);
    assert_eq!(entries.len(), 2);
    assert!(entries
        .iter()
        .any(|e| e.tlauncher && e.minecraft == "1.20.1"));
    assert!(entries
        .iter()
        .any(|e| !e.tlauncher && e.official && e.minecraft == "1.21"));
}

#[test]
fn selected_tlauncher_source_accepts_portable_and_nested_version_folders() {
    let t = Scratch::new("portable");
    let root = t.0.join("my folder");
    version(
        &root,
        "my-forge",
        json!({"id":"my-forge","clientVersion":"1.20.1","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0"}]}),
    );
    for folder in [
        &root,
        &root.join("versions"),
        &root.join("versions/my-forge"),
    ] {
        let mut rows = Vec::new();
        discover_for(
            Some(folder.clone()),
            Source::Tlauncher,
            false,
            &AtomicBool::new(false),
            |v| rows.extend(v),
        );
        assert_eq!(rows.len(), 1);
        let e = &rows[0];
        assert!(e.supported && e.matches(Source::Tlauncher));
        assert_eq!(e.minecraft, "1.20.1");
        assert_eq!(e.loader.as_ref().unwrap().version, "47.2.0");
    }
    discover_for(
        Some(t.0.join("missing")),
        Source::Official,
        false,
        &AtomicBool::new(false),
        |_| panic!("unexpected version"),
    );
}

#[test]
fn flattened_forge_and_renamed_tlauncher_profiles_keep_their_real_version() {
    let t = Scratch::new("flattened");
    let root = t.0.join("TLauncher");
    version(
        &root,
        "My Forge Pack",
        json!({"id":"forge-local","libraries":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0"}]}),
    );
    let entries = inspect(&root);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "My Forge Pack");
    assert_eq!(entries[0].minecraft, "1.20.1");
    assert_eq!(entries[0].loader.as_ref().unwrap().version, "47.2.0");
    assert!(entries[0].supported);
}
