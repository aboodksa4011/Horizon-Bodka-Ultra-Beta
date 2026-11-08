//! The pictures Home draws from the player's disk (screenshots, their blurred copies, the
//! background), made in the background at the size they are drawn and kept under a byte budget
//! (`horizon_launcher::pictures`).
//!
//! GPUI's `img(path)` keeps every picture it ever drew, at full size, until the launcher quits,
//! in RAM and in its sprite atlas: one 32768 × 16384 screenshot is 2 GiB, and Overview draws the
//! three newest as soon as Home opens (24 GiB reported on Linux, 2026-11-05). And a picture
//! wider than the GPU's texture limit made GPUI's atlas add a max-size texture on every frame.
//! Pictures are therefore made at the size the screen shows them, never above that limit
//! (`pictures::drawn_side`, `pictures::gpu_limit`).
use super::*;
use gpui::RenderImage;
use horizon_launcher::pictures::{self, Lru};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A screenshot tile's widest (Overview's first), in points; made at this times the screen's scale.
const THUMB_POINTS: f32 = 360.;
/// What the made pictures may take together, unless more than that is on screen at once.
const BUDGET: u64 = 384 << 20;
/// A picture drawn this recently is on screen: it stays even over the budget.
const GRACE: Duration = Duration::from_secs(2);
/// Pictures made at the same time; the others wait for the next frame.
const AT_ONCE: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Look {
    /// Fitted inside this many pixels square.
    Fit(u32),
    /// A fitted custom background, with blur performed once off the UI thread.
    Background(u32, u8),
    /// User-supplied built-in GIF, with original frame timing. A static first frame for reduced motion.
    BodkaFilm(u8, bool),
    FrostedFilm(u8, bool),
    FrostedPhoto(u8),
    /// Small and blurred (`glass_shot_blur`).
    Blurred,
}

type Key = (PathBuf, Look);

pub(super) struct Pictures {
    /// None: the file couldn't be read as a picture.
    made: Lru<Key, Option<Arc<RenderImage>>>,
    making: HashSet<Key>,
    /// Gone from `made`; freed from the window's sprite atlas on the next frame.
    retired: Vec<Arc<RenderImage>>,
    film_index: usize,
    film_deadline: Option<Instant>,
    film_wake_pending: bool,
}

impl Default for Pictures {
    fn default() -> Self {
        Pictures {
            made: Lru::new(BUDGET, GRACE),
            making: HashSet::new(),
            retired: Vec::new(),
            film_index: 0,
            film_deadline: None,
            film_wake_pending: false,
        }
    }
}

impl Pictures {
    fn retire(&mut self, gone: Vec<Option<Arc<RenderImage>>>) {
        self.retired.extend(gone.into_iter().flatten());
    }

    /// The picture if it is already made, without making it.
    pub(super) fn made(&self, path: &Path, look: Look) -> Option<Arc<RenderImage>> {
        self.made
            .peek(&(path.to_path_buf(), look))
            .cloned()
            .flatten()
    }

    /// Frees the pictures dropped since the last frame where GPUI keeps them too.
    pub(super) fn free_retired(&mut self, window: &mut Window) {
        for image in self.retired.drain(..) {
            let _ = window.drop_image(image);
        }
    }
}

impl Launcher {
    /// One clock drives the sharp wallpaper and every frosted panel at the GIF's own pace.
    pub(super) fn wallpaper_frame(
        &mut self,
        image: Option<&RenderImage>,
        cx: &mut Context<Self>,
    ) -> usize {
        let Some(image) = image.filter(|i| i.frame_count() > 1) else {
            self.pictures.film_index = 0;
            self.pictures.film_deadline = None;
            return 0;
        };
        let now = Instant::now();
        let index = &mut self.pictures.film_index;
        if self
            .pictures
            .film_deadline
            .is_some_and(|deadline| deadline <= now)
        {
            *index = (*index + 1) % image.frame_count();
            self.pictures.film_deadline = None;
        }
        let delay = Duration::from(image.delay(*index)).max(Duration::from_millis(16));
        let deadline = *self.pictures.film_deadline.get_or_insert(now + delay);
        let frame = *index;
        if !self.pictures.film_wake_pending {
            self.pictures.film_wake_pending = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(deadline.saturating_duration_since(Instant::now()))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.pictures.film_wake_pending = false;
                    cx.notify();
                });
            })
            .detach();
        }
        frame
    }

    /// The side to make a screenshot tile at, for this screen.
    pub(super) fn thumb_side(&self) -> u32 {
        pictures::drawn_side(THUMB_POINTS, self.scale_factor, pictures::gpu_limit())
    }

    /// The side to make a picture filling the window at (the viewer, the background): every
    /// pixel the screen can show, even on an 8K one, within the GPU's limit.
    pub(super) fn window_side(&self) -> u32 {
        pictures::drawn_side(
            self.window_long_side,
            self.scale_factor,
            pictures::gpu_limit(),
        )
    }

    /// The picture at `path` as `look`; None while it is being made, or when it can't be read
    /// (draw a plain fill then).
    pub(super) fn picture(
        &mut self,
        path: &Path,
        look: Look,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let now = Instant::now();
        let key = (path.to_path_buf(), look);
        if let Some(made) = self.pictures.made.get(&key, now).cloned() {
            if self.pictures.made.over_budget() {
                let gone = self.pictures.made.trim(now);
                self.pictures.retire(gone);
            }
            return made;
        }
        if self.pictures.making.len() < AT_ONCE && self.pictures.making.insert(key.clone()) {
            let task = cx.background_executor().spawn(async move {
                let image = make(&key.0, look);
                (key, image)
            });
            cx.spawn(async move |this, cx| {
                let (key, image) = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.pictures.making.remove(&key);
                    let bytes = image.as_ref().map_or(0, |i| {
                        (0..i.frame_count())
                            .map(|f| i.as_bytes(f).map_or(0, |b| b.len() as u64))
                            .sum()
                    });
                    let gone = this.pictures.made.insert(key, image, bytes, Instant::now());
                    this.pictures.retire(gone);
                    cx.notify();
                });
            })
            .detach();
        }
        None
    }
}

/// Makes the picture as GPUI draws it (BGRA, as its own loader does). Runs off the UI thread.
fn make(path: &Path, look: Look) -> Option<Arc<RenderImage>> {
    match look {
        Look::BodkaFilm(blur, moving) | Look::FrostedFilm(blur, moving) => {
            use image::AnimationDecoder;
            let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(
                include_bytes!("../assets/bodka/cherry-sunset.gif"),
            ))
            .ok()?;
            let frames = decoder
                .into_frames()
                .take(if moving { 75 } else { 1 })
                .map(|frame| {
                    let frame = frame.ok()?;
                    let delay = frame.delay();
                    let mut buffer = frame.into_buffer();
                    if matches!(look, Look::FrostedFilm(..)) {
                        let (w, h) = pictures::fitted(buffer.width(), buffer.height(), 192);
                        buffer = image::imageops::resize(
                            &buffer,
                            w,
                            h,
                            image::imageops::FilterType::Triangle,
                        );
                        buffer = frost(&buffer, blur);
                    } else if blur > 0 {
                        buffer = image::imageops::blur(&buffer, f32::from(blur.min(48)));
                    }
                    for pixel in buffer.pixels_mut() {
                        pixel.0.swap(0, 2);
                    }
                    Some(image::Frame::from_parts(buffer, 0, 0, delay))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Arc::new(RenderImage::new(frames)))
        }
        Look::Blurred => glass_shot_blur::blurred(path),
        Look::FrostedPhoto(blur) => {
            let image = pictures::load_fitted(path, 256.min(pictures::gpu_limit())).ok()?;
            let mut image = frost(&image, blur);
            for p in image.pixels_mut() {
                p.0.swap(0, 2);
            }
            Some(Arc::new(RenderImage::new([image::Frame::new(image)])))
        }
        Look::Fit(max) | Look::Background(max, _) => {
            // Never above the GPU's limit, whatever the caller asked: GPUI's atlas can't hold it.
            let mut image = pictures::load_fitted(path, max.min(pictures::gpu_limit())).ok()?;
            if let Look::Background(_, blur) = look {
                if blur > 0 {
                    image = image::imageops::blur(&image, f32::from(blur.min(48)));
                }
            }
            for p in image.pixels_mut() {
                p.0.swap(0, 2);
            }
            Some(Arc::new(RenderImage::new([image::Frame::new(image)])))
        }
    }
}

/// Blur only the small shared wallpaper texture. Text, icons and hit areas stay at screen resolution.
fn frost(image: &image::RgbaImage, background_blur: u8) -> image::RgbaImage {
    image::imageops::blur(image, 3.4 + f32::from(background_blur.min(48)) * 0.18)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_glass_blurs_detail_and_preserves_the_light_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wall.png");
        let image = image::RgbaImage::from_fn(192, 108, |x, _| {
            let light = if x < 96 { 40 } else { 180 };
            let grain = if x % 2 == 0 { 0 } else { 60 };
            image::Rgba([light + grain, 18, 10, 255])
        });
        image.save(&path).unwrap();
        let rendered = make(&path, Look::FrostedPhoto(0)).unwrap();
        let bytes = rendered.as_bytes(0).unwrap();
        let red = |x: usize| bytes[(50 * 192 + x) * 4 + 2];
        assert!(red(40).abs_diff(red(41)) < 5);
        assert!(red(152) > red(40) + 110);
        let film = make(Path::new("bodka:cherry-sunset"), Look::FrostedFilm(0, false)).unwrap();
        assert_eq!(film.frame_count(), 1);
        assert_eq!(u32::from(film.size(0).width), 192);
    }
}
