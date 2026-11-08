//! Explicit online smoke test. Requires an empty isolated launcher profile and public registries.
use horizon_launcher::{
    backend,
    browser::{self, java_instance, local_jar, shared_library::Profile, SourceId},
    instances, loaders,
};
use sha2::{Digest, Sha512};

#[test]
#[ignore = "uses the public Modrinth, Mojang and Fabric services and isolated game data"]
fn install_drop_share_restore_and_compare_real_files() {
    let scratch =
        std::path::PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("isolated profile"));
    assert!(
        backend::data_dir().starts_with(&scratch) && backend::game_home().starts_with(&scratch)
    );
    let src = browser::source(SourceId::Modrinth).unwrap();
    let made = instances::create_java("مكتبة الفحص", "1.20.1").unwrap();
    let loader = loaders::choose("fabric", "1.20.1", None).unwrap();
    let i = instances::edit(&made.id, |m| m.java.as_mut().unwrap().loader = Some(loader)).unwrap();
    let target = i.java.as_ref().unwrap().target();
    let project = src.project("modmenu").unwrap();
    let rows = java_instance::install(&*src, &target, &project, &i.home, &|_, _| {}).unwrap();
    let entry = rows.iter().find(|e| e.project == project.id).unwrap();
    let file = entry.path(&i.home).unwrap();
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(
        entry.checksum,
        format!("SHA-512:{:x}", Sha512::digest(&bytes))
    );
    let other = instances::create_java("Dropped", "1.20.1").unwrap();
    let installed = local_jar::install(&file, &target, &other.home).unwrap();
    assert!(installed.contains(&entry.title));
    assert_eq!(
        browser::installed::Installed::load(&other.home)
            .find(entry.source, &entry.project)
            .unwrap()
            .version,
        entry.version
    );
    let marker = i.home.join("saves").join("protected");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"unchanged").unwrap();
    java_instance::set_on(&i.home, entry, false).unwrap();
    let profile = Profile::parse(&Profile::of(&i, None).unwrap().code().unwrap()).unwrap();
    assert_eq!(profile.missing(&i), 0);
    let restored = profile.import(&|_, _| {}).unwrap();
    assert_ne!(restored.id, i.id);
    assert_eq!(restored.java, i.java);
    assert_eq!(profile.missing(&restored), 0);
    let restored_file = restored.home.join(format!("{}.disabled", entry.file));
    assert_eq!(std::fs::read(restored_file).unwrap(), bytes);
    assert_eq!(std::fs::read(marker).unwrap(), b"unchanged");
    eprintln!(
        "BODKA verified {} {} ({} bytes), {} pinned files",
        project.slug,
        entry.number,
        bytes.len(),
        profile.projects.len()
    );
}
