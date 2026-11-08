//! Run the original Horizon application's account command. Its registered OAuth client stays in its binary.
use super::*;
use crate::i18n::{tr, trf};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

const ASSET: &str = "https://github.com/horizon-rust/horizon/releases/download/launcher%400.2.47/horizon-launcher-windows-x64.zip";
const ARCHIVE_HASH: &str = "194fc71781ab0de636e408b981b50bdaa132593da3c7f5a2f817c719d3b2ff9f";
const EXE_HASH: &str = "bba8c3e8eafa432c6933c7ee2110a369b82c945382aed7f0932f13381c2b9487";
const EXE_LEN: u64 = 63_190_016;
const ARCHIVE_LIMIT: u64 = 100 << 20;

fn failure(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn digest(reader: &mut impl Read) -> io::Result<String> {
    let mut h = Sha256::new();
    io::copy(reader, &mut h)?;
    Ok(format!("{:x}", h.finalize()))
}
fn verified(path: &Path) -> bool {
    File::open(path).is_ok_and(|mut file| {
        file.metadata().is_ok_and(|m| m.len() == EXE_LEN)
            && digest(&mut file).is_ok_and(|s| s == EXE_HASH)
    })
}
fn executable() -> Result<PathBuf, String> {
    static LOCK: Mutex<()> = Mutex::new(());
    let _lock = LOCK.lock().map_err(failure)?;
    if let Ok(app) = std::env::current_exe() {
        let sidecar = app.with_file_name("Horizon").join("horizon-launcher.exe");
        if verified(&sidecar) {
            return Ok(sidecar);
        }
    }
    let home = crate::backend::data_dir()
        .join("official-horizon")
        .join("0.2.47");
    std::fs::create_dir_all(&home).map_err(failure)?;
    let target = home.join("horizon-launcher.exe");
    if verified(&target) {
        return Ok(target);
    }
    let mismatch = tr("Could not verify the official Horizon launcher.");
    let mut archive = tempfile::tempfile_in(&home).map_err(failure)?;
    let size = io::copy(
        &mut crate::backend::get(ASSET)?
            .into_reader()
            .take(ARCHIVE_LIMIT + 1),
        &mut archive,
    )
    .map_err(failure)?;
    archive.rewind().map_err(failure)?;
    if size > ARCHIVE_LIMIT || digest(&mut archive).map_err(failure)? != ARCHIVE_HASH {
        return Err(mismatch.into());
    }
    archive.seek(SeekFrom::Start(0)).map_err(failure)?;
    let mut zip = zip::ZipArchive::new(archive).map_err(failure)?;
    let mut entry = zip.by_name("horizon-launcher.exe").map_err(failure)?;
    if entry.size() != EXE_LEN {
        return Err(mismatch.into());
    }
    let mut out = tempfile::NamedTempFile::new_in(&home).map_err(failure)?;
    io::copy(&mut entry, &mut out).map_err(failure)?;
    out.flush().map_err(failure)?;
    if !verified(out.path()) {
        return Err(mismatch.into());
    }
    if out.persist(&target).is_err() && !verified(&target) {
        return Err(mismatch.into());
    }
    Ok(target)
}

fn command() -> Result<Command, String> {
    let mut c = Command::new(executable()?);
    c.creation_flags(0x08000000)
        .env_remove("HORIZON_MSA_CLIENT_ID")
        .env("HORIZON_LAUNCHER_HOME", crate::backend::data_dir())
        .env("HORIZON_HOME", crate::backend::game_home())
        .stdin(Stdio::null());
    Ok(c)
}

// Closing the launcher also closes its device-code request, including during Windows shutdown.
fn attach_request(child: &std::process::Child) -> io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    // Each handle lives for the entire call. Only the last close releases this private job.
    unsafe {
        let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(raw);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            raw,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
            || AssignProcessToJobObject(raw, child.as_raw_handle()) == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
}

fn code_from_line(line: &str) -> Option<DeviceCode> {
    let (prefix, user_code) = line
        .trim()
        .split_once(" in a browser and enter the code ")?;
    let (_, url) = prefix.rsplit_once("open ")?;
    if !matches!(
        url,
        "https://www.microsoft.com/link"
            | "https://microsoft.com/link"
            | "https://microsoft.com/devicelogin"
            | "https://www.microsoft.com/devicelogin"
    ) || !(6..=16).contains(&user_code.len())
        || !user_code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return None;
    }
    Some(DeviceCode {
        user_code: user_code.into(),
        verification_uri: url.into(),
        interval: 0,
        expires_at: 0,
        device_code: String::new(),
    })
}

pub(super) fn sign_in(
    expected: Option<&Account>,
    cancel: &AtomicBool,
    show: impl Fn(SignInState),
) -> Result<Account, String> {
    let mut c = command()?;
    if cancel.load(Ordering::Relaxed) {
        return Err(tr("Sign-in cancelled.").into());
    }
    c.args(["account", "sign-in"]);
    if let Some(a) = expected {
        c.args(["--for", &a.uuid]);
    }
    let mut child = c
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failure)?;
    let _request_job = attach_request(&child).map_err(|e| {
        let _ = child.kill();
        let _ = child.wait();
        e.to_string()
    })?;
    let err = child
        .stderr
        .take()
        .ok_or(tr("Could not start official Horizon sign-in."))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if let Some(code) = code_from_line(&line) {
                let _ = tx.send(code);
            }
        }
    });
    let started = Instant::now();
    loop {
        if let Ok(code) = rx.try_recv() {
            show(SignInState::Code(code));
        }
        if cancel.load(Ordering::Relaxed) || started.elapsed() >= Duration::from_secs(1200) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(tr("Sign-in cancelled or expired.").into());
        }
        if let Some(status) = child.try_wait().map_err(failure)? {
            if !status.success() {
                return Err(tr("Official Horizon could not finish sign-in. Check the connection and account ownership, then try again.").into());
            }
            let accounts = Accounts::load();
            let signed = expected.and_then(|a| accounts.find(&a.uuid)).or_else(|| {
                if expected.is_none() {
                    accounts.selected()
                } else {
                    None
                }
            });
            return signed
                .cloned()
                .ok_or_else(|| tr("Horizon did not return a linked account.").into());
        }
        std::thread::sleep(Duration::from_millis(125));
    }
}

pub(super) fn applicable(ep: &Endpoints, id: Option<&str>, storage: &dyn SecretStore) -> bool {
    id.is_none()
        && ep.microsoft == Endpoints::live().microsoft
        && std::ptr::addr_eq(storage, store())
}
pub(super) fn refresh(uuid: &str) -> Result<session::Status, String> {
    let out = command()?
        .args(["account", "refresh", uuid, "--json"])
        .stderr(Stdio::null())
        .output()
        .map_err(failure)?;
    if !out.status.success() {
        return Err(tr("Could not renew the account through official Horizon.").into());
    }
    serde_json::from_slice(&out.stdout).map_err(|_| {
        trf(
            "Could not read the response from {app}.",
            &[("app", "Horizon")],
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_accepts_a_microsoft_user_code_from_horizon() {
        let line = "To sign in, open https://www.microsoft.com/link in a browser and enter the code ABC123XYZ";
        assert!(code_from_line(line).is_some());
        assert!(code_from_line(&line.replace("www.microsoft.com", "example.org")).is_none());
        assert!(code_from_line(&line.replace("ABC123XYZ", "/tmp/path")).is_none());
    }
    #[test]
    #[ignore = "opens a real, unapproved Microsoft device request then cancels it"]
    fn official_horizon_live_device_request() {
        assert!(std::env::var_os("BODKA_TEST_SCRATCH").is_some());
        let cancel = AtomicBool::new(false);
        let result = sign_in(None, &cancel, |state| {
            if matches!(state, SignInState::Code(_)) {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert!(result.is_err());
        assert!(
            cancel.load(Ordering::Relaxed),
            "Official Horizon must supply a genuine Microsoft device code"
        );
        assert!(Accounts::load().accounts.is_empty());
    }
}
