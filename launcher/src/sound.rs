//! Short, original PCM cues. One worker at a time; no UI blocking and no queued clicks.
use horizon_launcher::bodka_settings::Settings;
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum Event {
    Startup,
    Click,
    Version,
    Complete,
    Notice,
    Error,
}
thread_local! {
    static OPTIONS: Cell<(bool,u8,bool)> = const { Cell::new((false,25,true)) };
    static INITIALIZED: Cell<bool> = const { Cell::new(false) };
}
static BUSY: AtomicBool = AtomicBool::new(false);
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

pub fn configure(settings: &Settings) {
    OPTIONS.set((
        settings.sound,
        settings.volume.min(100),
        settings.soft_sound,
    ));
    if !INITIALIZED.replace(true) {
        play(Event::Startup);
    }
}

pub fn play(event: Event) {
    let (enabled, volume, soft) = OPTIONS.get();
    if !enabled || volume == 0 || std::env::var_os("HORIZON_NO_AUDIO").is_some() {
        return;
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| at.elapsed() < Duration::from_millis(180))
        || BUSY.swap(true, Ordering::AcqRel)
    {
        return;
    }
    *last = Some(Instant::now());
    drop(last);
    std::thread::spawn(move || {
        let bytes = wave(event, volume, soft);
        #[cfg(windows)]
        {
            use windows_sys::Win32::Media::Audio::{
                PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC,
            };
            // SAFETY: SND_MEMORY accepts a complete RIFF buffer. Synchronous playback on this
            // worker keeps it alive until Windows has finished reading it.
            unsafe {
                PlaySoundW(
                    bytes.as_ptr().cast(),
                    std::ptr::null_mut(),
                    SND_MEMORY | SND_NODEFAULT | SND_SYNC,
                );
            }
        }
        #[cfg(not(windows))]
        let _ = bytes;
        BUSY.store(false, Ordering::Release);
    });
}

fn wave(event: Event, volume: u8, soft: bool) -> Vec<u8> {
    let (frequency, millis) = match event {
        Event::Startup => (440., 180),
        Event::Click => (540., 35),
        Event::Version => (620., 70),
        Event::Complete => (780., 170),
        Event::Notice => (650., 110),
        Event::Error => (220., 140),
    };
    let samples = 22_050 * millis / 1000;
    let mut bytes = Vec::with_capacity(44 + samples * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + samples as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&22_050u32.to_le_bytes());
    bytes.extend_from_slice(&44_100u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(samples as u32 * 2).to_le_bytes());
    for i in 0..samples {
        let t = i as f32 / 22_050.;
        let envelope = (std::f32::consts::PI * i as f32 / samples as f32)
            .sin()
            .powi(2);
        let freq = if soft { frequency * 0.75 } else { frequency };
        let amplitude = if soft { 0.10 } else { 0.16 } * f32::from(volume.min(100)) / 100.;
        let sample =
            ((std::f32::consts::TAU * freq * t).sin() * envelope * amplitude * f32::from(i16::MAX))
                as i16;
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_cues_are_complete_quiet_pcm_and_zero_volume_is_silent() {
        for event in [
            Event::Startup,
            Event::Click,
            Event::Version,
            Event::Complete,
            Event::Notice,
            Event::Error,
        ] {
            let bytes = wave(event, 100, true);
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(
                u32::from_le_bytes(bytes[40..44].try_into().unwrap()) as usize,
                bytes.len() - 44
            );
            let peak = bytes[44..]
                .as_chunks::<2>().0.iter()
                .map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs())
                .max()
                .unwrap();
            assert!(peak > 0 && peak < 3300);
            assert!(wave(event, 0, false)[44..].iter().all(|b| *b == 0));
        }
    }
}
