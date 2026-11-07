//! Resumable public Horizon transfers. Authentication and signed-release verification remain
//! in backend::install. The cache is never treated as trusted until its full digest matches.
use crate::{
    backend,
    i18n::{tr, trf},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    CheckingSource,
    Downloading,
    Paused,
    Verifying,
    Installing,
    Ready,
    Cancelled,
    Failed,
    Interrupted,
}
impl State {
    pub fn label(self) -> &'static str {
        match self {
            Self::CheckingSource => "Checking official source",
            Self::Downloading => "Downloading",
            Self::Paused => "Paused",
            Self::Verifying => "Verifying files",
            Self::Installing => "Installing",
            Self::Ready => "Ready",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted download",
        }
    }
    pub fn controllable(self) -> bool {
        matches!(
            self,
            Self::CheckingSource | Self::Downloading | Self::Paused
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub tag: String,
    pub file: String,
    pub total: u64,
    pub downloaded: u64,
    pub bytes_per_second: f64,
    pub state: State,
    pub error: Option<String>,
}
impl Transfer {
    pub fn fraction(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.downloaded as f64 / self.total as f64).clamp(0., 1.) as f32)
    }
    pub fn seconds_left(&self) -> Option<u64> {
        (self.state == State::Downloading && self.bytes_per_second > 0. && self.total > 0).then(
            || {
                ((self.total.saturating_sub(self.downloaded)) as f64 / self.bytes_per_second).ceil()
                    as u64
            },
        )
    }
}

#[derive(Clone, Default)]
pub struct Control(Arc<AtomicU8>);
impl Control {
    pub fn pause(&self) {
        let _ = self
            .0
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst);
    }
    pub fn resume(&self) {
        let _ = self
            .0
            .compare_exchange(1, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
    pub fn cancel(&self) {
        self.0.store(2, Ordering::SeqCst);
    }
    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst) == 2
    }
    fn wait(&self) -> Result<(), String> {
        while self.0.load(Ordering::SeqCst) == 1 {
            std::thread::sleep(Duration::from_millis(100));
        }
        if self.cancelled() {
            Err(tr("Download cancelled").into())
        } else {
            Ok(())
        }
    }
}

type Observer = Arc<dyn Fn(Transfer) + Send + Sync>;
struct Active {
    transfer: Transfer,
    control: Control,
    observer: Observer,
    saved: Instant,
}
thread_local! { static ACTIVE:RefCell<Option<Active>>=const{RefCell::new(None)}; }

pub fn directory() -> PathBuf {
    backend::data_dir().join("downloads")
}
fn job_key(tag: &str) -> String {
    horizon_release_sign::sha256_hex(tag.as_bytes())
}
fn persist(transfer: &Transfer) -> Result<(), String> {
    let root = directory();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let path = root.join(format!("{}.json", job_key(&transfer.tag)));
    let tmp = path.with_extension(format!("json.{}.tmp", std::process::id()));
    let data = serde_json::to_vec(transfer).map_err(|e| e.to_string())?;
    let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    f.write_all(&data).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    drop(f);
    fs::rename(tmp, path).map_err(|e| e.to_string())
}

pub fn history() -> Vec<Transfer> {
    history_at(&directory())
}
fn history_at(directory: &Path) -> Vec<Transfer> {
    let mut out = Vec::new();
    for item in fs::read_dir(directory).into_iter().flatten().flatten() {
        let path = item.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(data) = fs::read(&path) else {
            continue;
        };
        let Ok(mut t) = serde_json::from_slice::<Transfer>(&data) else {
            continue;
        };
        if matches!(
            t.state,
            State::CheckingSource
                | State::Downloading
                | State::Paused
                | State::Verifying
                | State::Installing
        ) {
            t.state = State::Interrupted;
        }
        t.bytes_per_second = 0.;
        out.push(t);
    }
    out.sort_by(|a, b| backend::compare_versions(&b.tag, &a.tag));
    out
}

fn publish(mut change: impl FnMut(&mut Transfer), force: bool) {
    ACTIVE.with_borrow_mut(|active| {
        if let Some(a) = active {
            change(&mut a.transfer);
            if force || a.saved.elapsed() >= Duration::from_secs(1) {
                // A failed journal write is visible without discarding the downloaded bytes.
                if let Err(e) = persist(&a.transfer) {
                    a.transfer.error = Some(trf(
                        "Could not save download progress: {error}",
                        &[("error", &e)],
                    ));
                }
                a.saved = Instant::now();
            }
            (a.observer)(a.transfer.clone());
        }
    });
}

pub(crate) fn stage(state: State) {
    publish(
        |t| {
            t.state = state;
            t.bytes_per_second = 0.;
        },
        true,
    );
}
pub(crate) fn active() -> bool {
    ACTIVE.with_borrow(Option::is_some)
}

pub fn run(
    release: &backend::Release,
    control: Control,
    observer: impl Fn(Transfer) + Send + Sync + 'static,
    progress: impl Fn(f32),
) -> Result<(), String> {
    let (url, size) = release
        .asset
        .as_ref()
        .ok_or_else(|| tr("No build for this platform").to_string())?;
    validate_source(url)?;
    let transfer = Transfer {
        tag: release.tag.clone(),
        file: url.rsplit('/').next().unwrap_or_default().into(),
        total: *size,
        downloaded: 0,
        bytes_per_second: 0.,
        state: State::CheckingSource,
        error: None,
    };
    persist(&transfer)?;
    ACTIVE.with_borrow_mut(|a| {
        *a = Some(Active {
            transfer,
            control: control.clone(),
            observer: Arc::new(observer),
            saved: Instant::now(),
        })
    });
    stage(State::CheckingSource);
    let result = control
        .wait()
        .and_then(|()| backend::install(release, progress));
    publish(
        |t| {
            t.bytes_per_second = 0.;
            t.state = match &result {
                Ok(()) => State::Ready,
                Err(_) if control.cancelled() => State::Cancelled,
                Err(_) => State::Failed,
            };
            t.error = result.as_ref().err().cloned();
        },
        true,
    );
    ACTIVE.with_borrow_mut(|a| *a = None);
    result
}

fn validate_source(url: &str) -> Result<(), String> {
    if !url.starts_with("https://github.com/horizon-rust/horizon/releases/download/")
        || url.contains(['?', '#', '\\'])
    {
        return Err(tr("The download source is not an official Horizon release.").into());
    }
    Ok(())
}

pub(crate) fn download(url: &str, total: u64, sha256: &str, to: &Path) -> Result<(), String> {
    validate_source(url)?;
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(tr("Invalid signed checksum").into());
    }
    let root = directory().join("cache");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let part = root.join(format!("{sha256}.part"));
    let control = ACTIVE
        .with_borrow(|a| a.as_ref().map(|a| a.control.clone()))
        .unwrap_or_default();
    let done = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let required = total
        .saturating_mul(5)
        .saturating_add(64 << 20)
        .saturating_sub(done);
    if let Some(free) = free_space(&root) {
        if free < required {
            return Err(trf(
                "Not enough disk space. Required: {required} MB; available: {available} MB.",
                &[
                    ("required", &(required >> 20).to_string()),
                    ("available", &(free >> 20).to_string()),
                ],
            ));
        }
    }
    receive(url, total, &part, &control, |done, total, rate| {
        publish(
            |t| {
                t.downloaded = done;
                t.total = total;
                t.bytes_per_second = rate;
                t.state = State::Downloading;
            },
            false,
        )
    })?;
    control.wait()?;
    stage(State::Verifying);
    verify_archive(
        &part,
        sha256,
        to,
        url.rsplit('/').next().unwrap_or_default(),
    )
}

fn verify_archive(part: &Path, sha256: &str, to: &Path, name: &str) -> Result<(), String> {
    let mut hash = horizon_release_sign::Sha256::default();
    let mut file = fs::File::open(part).map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    drop(file);
    if let Err(e) = horizon_release_sign::check(name, sha256, &hash.hex()) {
        let _ = fs::remove_file(part);
        return Err(e);
    }
    fs::copy(part, to).map_err(|e| e.to_string())?;
    Ok(())
}

fn response_start(status: u16, range: Option<&str>, offset: u64) -> Result<u64, String> {
    if status == 200 {
        return Ok(0);
    }
    if status == 206 {
        let start = range
            .and_then(|r| r.strip_prefix("bytes "))
            .and_then(|r| r.split_once('-'))
            .and_then(|(n, _)| n.parse::<u64>().ok());
        if start == Some(offset) {
            return Ok(offset);
        }
    }
    Err(tr("The server returned an invalid resume range.").into())
}

fn receive(
    url: &str,
    expected: u64,
    path: &Path,
    control: &Control,
    notify: impl Fn(u64, u64, f64),
) -> Result<(), String> {
    loop {
        control.wait()?;
        let offset = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if expected > 0 && offset == expected {
            notify(offset, expected, 0.);
            return Ok(());
        }
        let builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(5));
        let agent = crate::proxy::agent(builder, url).build();
        let mut request = agent
            .get(url)
            .set("Accept-Encoding", "identity")
            .set("User-Agent", "BODKA-11");
        if offset > 0 {
            request = request.set("Range", &format!("bytes={offset}-"));
        }
        let response = match request.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(416, _)) if offset > 0 => {
                fs::File::create(path).map_err(|e| e.to_string())?;
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let start = response_start(response.status(), response.header("Content-Range"), offset)?;
        let total = if expected > 0 {
            expected
        } else {
            response
                .header("Content-Length")
                .and_then(|n| n.parse::<u64>().ok())
                .map_or(0, |n| n + start)
        };
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(start == 0)
            .append(start > 0)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut reader = response.into_reader();
        let mut done = start;
        let begun = Instant::now();
        let mut shown = Instant::now();
        let mut buffer = vec![0; 1 << 16];
        notify(done, total, 0.);
        loop {
            if control.cancelled() {
                f.sync_all().map_err(|e| e.to_string())?;
                return Err(tr("Download cancelled").into());
            }
            if control.0.load(Ordering::SeqCst) == 1 {
                f.sync_all().map_err(|e| e.to_string())?;
                drop(reader);
                drop(f);
                stage(State::Paused);
                control.wait()?;
                break;
            }
            let n = reader.read(&mut buffer).map_err(|e| {
                trf(
                    "Download interrupted: {error}",
                    &[("error", &e.to_string())],
                )
            })?;
            if n == 0 {
                f.sync_all().map_err(|e| e.to_string())?;
                if total > 0 && done != total {
                    return Err(tr("The download ended before the complete file arrived.").into());
                }
                notify(done, total, 0.);
                return Ok(());
            }
            f.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if total > 0 && done > total {
                return Err(tr("The download exceeds its official size.").into());
            }
            if shown.elapsed() >= Duration::from_millis(200) {
                notify(
                    done,
                    total,
                    (done - start) as f64 / begun.elapsed().as_secs_f64().max(0.001),
                );
                shown = Instant::now();
            }
        }
    }
}

pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut bytes = 0;
        // SAFETY: the path is NUL terminated, and the output is a valid u64.
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut bytes,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Downloads a signed official release; requires an explicitly isolated launcher home"]
    fn official_horizon_archive_download_and_install() {
        assert_eq!(
            std::env::var("BODKA_TEST_OFFICIAL_DOWNLOAD").as_deref(),
            Ok("1")
        );
        let home = std::env::var("HORIZON_LAUNCHER_HOME").expect("isolated launcher home");
        assert!(
            home.contains("bodka11-network-check"),
            "never run against real player data"
        );
        let release = backend::releases()
            .unwrap()
            .into_iter()
            .find(|r| r.asset.is_some())
            .expect("available official build");
        let begun = Instant::now();
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let states = observed.clone();
        run(
            &release,
            Control::default(),
            move |t| {
                let mut states = states.lock().unwrap();
                if states.last() != Some(&t.state) {
                    eprintln!("{:?}: {} / {} bytes", t.state, t.downloaded, t.total);
                    states.push(t.state);
                }
            },
            |_| {},
        )
        .unwrap();
        assert!(backend::installed().contains(&release.tag));
        let states = observed.lock().unwrap();
        for expected in [
            State::CheckingSource,
            State::Downloading,
            State::Verifying,
            State::Installing,
            State::Ready,
        ] {
            assert!(states.contains(&expected), "missing {expected:?}");
        }
        eprintln!(
            "OFFICIAL_INSTALL {} {:.2}s",
            release.tag,
            begun.elapsed().as_secs_f64()
        );
    }
    use std::net::TcpListener;
    use std::sync::mpsc;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "bodka-transfer-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            for file in fs::read_dir(&self.0).into_iter().flatten().flatten() {
                let _ = fs::remove_file(file.path());
            }
            let _ = fs::remove_dir(&self.0);
        }
    }
    // Test-only HTTP fixture: production entry points still reject non-official sources.
    fn server(
        data: Vec<u8>,
        ranges: bool,
        slow: bool,
    ) -> (
        String,
        mpsc::Receiver<u64>,
        Arc<AtomicU8>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicU8::new(0));
        let stopped = stop.clone();
        let handle = std::thread::spawn(move || {
            let begun = Instant::now();
            while stopped.load(Ordering::SeqCst) == 0 && begun.elapsed() < Duration::from_secs(15) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(3));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") && header.len() < 8192 {
                    if stream.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    header.push(byte[0]);
                }
                let header = String::from_utf8_lossy(&header).to_lowercase();
                let offset = header
                    .lines()
                    .find_map(|l| l.strip_prefix("range: bytes="))
                    .and_then(|n| n.split('-').next())
                    .and_then(|n| n.parse::<u64>().ok())
                    .unwrap_or(0);
                let _ = tx.send(offset);
                let start = if ranges { offset as usize } else { 0 };
                let response = if start > 0 {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\n",
                        data.len() - 1,
                        data.len()
                    )
                } else {
                    "HTTP/1.1 200 OK\r\n".into()
                };
                if write!(
                    stream,
                    "{response}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    data.len() - start
                )
                .is_err()
                {
                    continue;
                }
                for chunk in data[start..].chunks(16 * 1024) {
                    if stream.write_all(chunk).is_err() {
                        break;
                    }
                    if slow {
                        std::thread::sleep(Duration::from_millis(12));
                    }
                }
            }
        });
        (url, rx, stop, handle)
    }

    #[test]
    fn pause_cancel_and_retry_preserve_real_bytes_and_resume_with_range() {
        let scratch = Scratch::new();
        let path = scratch.0.join("archive.part");
        let data: Vec<_> = (0..524288).map(|n| (n % 251) as u8).collect();
        let (url, requests, stop, worker) = server(data.clone(), true, true);
        let control = Control::default();
        let paused = std::sync::atomic::AtomicBool::new(false);
        receive(&url, data.len() as u64, &path, &control, |done, _, _| {
            if done > 0 && !paused.swap(true, Ordering::SeqCst) {
                control.pause();
                let resume = control.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(80));
                    resume.resume();
                });
            }
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert!(
            requests.try_iter().any(|offset| offset > 0),
            "pause must resume from cached bytes"
        );
        fs::write(&path, []).unwrap();
        let cancel = Control::default();
        assert!(
            receive(&url, data.len() as u64, &path, &cancel, |done, _, _| {
                if done > 0 {
                    cancel.cancel();
                }
            })
            .is_err()
        );
        let retained = fs::metadata(&path).unwrap().len();
        assert!(retained > 0 && retained < data.len() as u64);
        receive(
            &url,
            data.len() as u64,
            &path,
            &Control::default(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        stop.store(1, Ordering::SeqCst);
        worker.join().unwrap();
    }

    #[test]
    fn a_server_ignoring_range_restarts_and_a_bad_digest_never_installs() {
        let scratch = Scratch::new();
        let path = scratch.0.join("archive.part");
        let target = scratch.0.join("installed.zip");
        let data = b"complete official-shaped fixture".to_vec();
        fs::write(&path, &data[..7]).unwrap();
        let (url, _, stop, worker) = server(data.clone(), false, false);
        receive(
            &url,
            data.len() as u64,
            &path,
            &Control::default(),
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert!(verify_archive(&path, &"0".repeat(64), &target, "test.zip").is_err());
        assert!(!target.exists());
        assert!(!path.exists());
        fs::write(&path, &data).unwrap();
        verify_archive(
            &path,
            &horizon_release_sign::sha256_hex(&data),
            &target,
            "test.zip",
        )
        .unwrap();
        assert_eq!(fs::read(&target).unwrap(), data);
        stop.store(1, Ordering::SeqCst);
        worker.join().unwrap();
    }

    #[test]
    fn restarting_restores_interrupted_transfers_without_a_stale_speed() {
        let scratch = Scratch::new();
        let t = Transfer {
            tag: "v1".into(),
            file: "game.zip".into(),
            total: 100,
            downloaded: 40,
            bytes_per_second: 30.,
            state: State::Downloading,
            error: None,
        };
        fs::write(scratch.0.join("job.json"), serde_json::to_vec(&t).unwrap()).unwrap();
        let restored = history_at(&scratch.0);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].downloaded, 40);
        assert_eq!(restored[0].state, State::Interrupted);
        assert_eq!(restored[0].bytes_per_second, 0.);
        assert_eq!(restored[0].seconds_left(), None);
    }
    #[test]
    fn ranges_never_append_an_entire_response_or_the_wrong_slice() {
        assert_eq!(response_start(200, None, 42).unwrap(), 0);
        assert_eq!(
            response_start(206, Some("bytes 42-99/100"), 42).unwrap(),
            42
        );
        assert!(response_start(206, Some("bytes 0-99/100"), 42).is_err());
        assert!(response_start(206, None, 42).is_err());
    }
    #[test]
    fn sources_are_the_official_repository_only() {
        assert!(validate_source(
            "https://github.com/horizon-rust/horizon/releases/download/v1/game.zip"
        )
        .is_ok());
        for bad in [
            "http://github.com/horizon-rust/horizon/releases/download/v1/game.zip",
            "https://github.com.evil.test/horizon-rust/horizon/releases/download/v1/game.zip",
            "https://tlauncher.org/game.zip",
        ] {
            assert!(validate_source(bad).is_err());
        }
    }
    #[test]
    fn paused_time_and_unknown_sizes_do_not_invent_an_eta() {
        let mut t = Transfer {
            tag: "test".into(),
            file: "file".into(),
            total: 100,
            downloaded: 25,
            bytes_per_second: 25.,
            state: State::Downloading,
            error: None,
        };
        assert_eq!(t.seconds_left(), Some(3));
        assert_eq!(t.fraction(), Some(0.25));
        t.state = State::Paused;
        assert_eq!(t.seconds_left(), None);
        t.total = 0;
        assert_eq!(t.fraction(), None);
    }
}
