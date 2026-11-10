use super::Source;

/// A launcher registered in the current Windows user's Start menu.
#[derive(Debug)]
pub struct InstalledLauncher {
    pub source: Source,
    #[cfg(windows)]
    target: Target,
}

#[cfg(windows)]
#[derive(Debug)]
enum Target {
    Registered(String),
    Program(std::path::PathBuf),
}

impl InstalledLauncher {
    #[cfg(windows)]
    pub fn detect() -> Vec<Self> {
        let mut apps = Self::registered();
        let program_dirs = ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(std::path::PathBuf::from)
            .chain(dirs::data_local_dir().map(|p| p.join("Programs")));
        for dir in program_dirs {
            for (source, relative) in [
                (Source::Tlauncher, "TLauncher/TLauncher.exe"),
                (Source::Official, "Minecraft Launcher/MinecraftLauncher.exe"),
            ] {
                let path = dir.join(relative);
                if !apps.iter().any(|a| a.source == source) && super::regular(&path) {
                    apps.push(Self {
                        source,
                        target: Target::Program(path),
                    });
                }
            }
        }
        apps
    }

    #[cfg(windows)]
    fn registered() -> Vec<Self> {
        use std::{os::windows::process::CommandExt, process::Command};
        let Some(root) = std::env::var_os("SystemRoot") else {
            return Vec::new();
        };
        let shell =
            std::path::Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        // Only enumerate the two launcher registrations. Bedrock has a different AppID.
        let result = Command::new(shell)
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); Get-StartApps | Where-Object { $_.AppID -eq 'Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft' -or $_.Name -eq 'Minecraft Launcher' -or $_.Name -match '^TLauncher(?: [0-9.]+)?$' } | ForEach-Object { if ($_.Name -match '^TLauncher') { 'tl|' + $_.AppID } else { 'mc|' + $_.AppID } }"])
            .creation_flags(0x08000000)
            .output();
        let Ok(out) = result else { return Vec::new() };
        if !out.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let (kind, app_id) = line.trim().split_once('|')?;
                let source = match kind {
                    "tl" => Source::Tlauncher,
                    "mc" => Source::Official,
                    _ => return None,
                };
                (!app_id.is_empty() && !app_id.chars().any(char::is_control)).then(|| Self {
                    source,
                    target: Target::Registered(app_id.to_owned()),
                })
            })
            .collect()
    }

    #[cfg(windows)]
    pub fn open(&self) -> std::io::Result<()> {
        use std::process::Command;
        match &self.target {
            Target::Registered(id) => {
                let windows = std::env::var_os("SystemRoot").ok_or(std::io::ErrorKind::NotFound)?;
                Command::new(std::path::Path::new(&windows).join("explorer.exe"))
                    .arg(format!(r"shell:AppsFolder\{id}"))
                    .spawn()
                    .map(drop)
            }
            Target::Program(path) => Command::new(path).spawn().map(drop),
        }
    }

    #[cfg(not(windows))]
    pub fn detect() -> Vec<Self> {
        Vec::new()
    }
    #[cfg(not(windows))]
    pub fn open(&self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}
