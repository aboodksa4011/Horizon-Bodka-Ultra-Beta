//! BODKA's presentation identity. Keep protocol names and upstream release identifiers intact.
pub const APP_NAME: &str = "𝓗𝓸𝓻𝓲𝔃𝓸𝓷 𝓑𝓸𝓭𝓴𝓪 𝓤𝓵𝓽𝓻𝓪";
/// ASCII paths keep the native Rust build toolchain usable on Windows.
pub const INSTALL_FOLDER: &str = "BODKA Horizon Ultra";
pub const APP_ID: &str = "bodka-horizon-ultra";
/// Upstream packages are signed by Horizon, but would replace this distribution's launcher.
/// Enable only after BODKA has its own verified release feed and signing infrastructure.
pub const UPSTREAM_SELF_UPDATE: bool = false;

pub fn default_language() -> Option<crate::i18n::Language> {
    Some(crate::i18n::Language::Arabic)
}

pub fn default_accent() -> crate::themes::Accent {
    crate::themes::Accent::from_rgb(0xa77b52)
}

/// Seed new game folders, including downloaded upstream/Java builds. A player's existing
/// options (and any language they deliberately selected) are never replaced or rewritten.
pub fn initialize_game_language(home: &std::path::Path) -> std::io::Result<bool> {
    use std::io::Write;
    std::fs::create_dir_all(home)?;
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(home.join("options.txt"))
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    file.write_all(b"lang:ar_sa\n")?;
    file.sync_all()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_games_start_in_arabic_and_existing_options_are_never_rewritten() {
        let root = std::env::temp_dir().join(format!(
            "bodka-language-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(super::initialize_game_language(&root).unwrap());
        let path = root.join("options.txt");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "lang:ar_sa\n");
        let existing = "lang:en_us\r\nrenderDistance:16\r\n";
        std::fs::write(&path, existing).unwrap();
        assert!(!super::initialize_game_language(&root).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
