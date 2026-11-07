//! Opt-in read-only discovery and import smoke checks against real local installations.
use horizon_launcher::{backend, dual, instances};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};

#[test]
#[ignore = "read-only scan of the host's mounted local drives"]
fn scan_local_drives() {
    let begin = Instant::now();
    let mut count = 0;
    let mut official = 0;
    let mut tl = 0;
    let summary = dual::discover(None, &AtomicBool::new(false), |items| {
        count += items.len();
        official += items.iter().filter(|e| e.official).count();
        tl += items.iter().filter(|e| e.tlauncher).count();
    });
    eprintln!("DUAL_SCAN drives={} rows={count} official={official} tlauncher={tl} skipped={} seconds={:.2}",
              summary.volumes,summary.skipped,begin.elapsed().as_secs_f64());
    assert!(summary.volumes > 0);
}

#[test]
#[ignore = "requires BODKA_TEST_GAME_FOLDER, an isolated empty profile and Mojang's public catalog"]
fn import_real_local_version_and_reopen_as_guest() {
    let root =
        PathBuf::from(std::env::var_os("BODKA_TEST_GAME_FOLDER").expect("local installation"));
    let data = backend::data_dir();
    let scratch =
        PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("isolated test profile"));
    assert!(data.starts_with(&scratch) && backend::game_home().starts_with(&scratch));
    assert!(
        !data.join("accounts.json").exists(),
        "use an empty guest profile"
    );
    let id = std::env::var("BODKA_TEST_VERSION").unwrap_or_else(|_| "1.21.1".into());
    let list = dual::inspect(&root);
    let entry = list
        .iter()
        .find(|e| e.minecraft == id && e.loader.is_none() && e.mods.is_empty())
        .expect("installed vanilla version with no mods");
    let first = dual::import(entry).unwrap();
    let second = dual::import(entry).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(
        instances::find(&first.id).unwrap().java.unwrap().minecraft,
        id
    );
    assert!(dual::imported()
        .iter()
        .any(|(origin, i)| origin == &entry.origin && i == &first.id));
    let ownership_error = Some(horizon_launcher::account::NoOwner.to_string());
    assert_eq!(
        backend::java_game(&first, None, &|_| {}).err(),
        ownership_error
    );
    assert_eq!(backend::repair_java(&first, &|_| {}).err(), ownership_error);
    assert!(
        !data.join("minecraft/versions").exists(),
        "guest must not obtain game files"
    );
    eprintln!(
        "DUAL_IMPORT version={id} rows={} unique_instance=1 guest_launch_and_repair_blocked=true",
        list.len()
    );
}
