use super::*;
use gpui::AnyWindowHandle;
use std::sync::Arc;

/// Exercise the route that formerly inserted an opaque page above the wallpaper.
#[test]
#[ignore = "requires Windows graphics, a network connection and an isolated Forge library"]
fn catalogue_keeps_the_frosted_wallpaper_after_navigation() {
    use crate::picture_cache::Look;
    let scratch =
        std::path::PathBuf::from(std::env::var_os("BODKA_TEST_SCRATCH").expect("scratch"));
    assert!(
        backend::data_dir().starts_with(&scratch) && backend::game_home().starts_with(&scratch)
    );
    let seen = Arc::new(Mutex::new(false));
    let checked = seen.clone();
    gpui_platform::application()
        .with_assets(blocks::Assets)
        .run(move |cx| {
            let bounds = Bounds::centered(None, size(px(1344.), px(840.)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        show: true,
                        focus: false,
                        ..Default::default()
                    },
                    |window, cx| {
                        cx.new(|cx| {
                            let mut app = Launcher::new(window, cx);
                            app.opening = None;
                            app.collection.consent.answer(Default::default());
                            app.show_page(Page::Library);
                            app.open_essential_catalog();
                            app
                        })
                    },
                )
                .unwrap();
            cx.spawn(async move |cx| {
                for attempt in 0..240 {
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                    let captured = handle
                        .update(cx, |app, w, cx| {
                            w.refresh();
                            if app.page != Page::Mods
                                || app.mods.results.is_empty()
                                || !app.library.browsing_library()
                            {
                                return None;
                            }
                            // Give registry icons time to arrive before recording the catalogue.
                            if attempt < 60
                                && app
                                    .mods
                                    .results
                                    .iter()
                                    .take(6)
                                    .filter_map(|p| p.icon.as_ref())
                                    .any(|url| {
                                        horizon_launcher::browser::images::cached(
                                            &backend::data_dir(),
                                            url,
                                            horizon_launcher::browser::images::ICON,
                                        )
                                        .is_none()
                                    })
                            {
                                return None;
                            }
                            let frost = app.pictures.made(
                                std::path::Path::new("bodka:cherry-sunset"),
                                Look::FrostedFilm(0, false),
                            )?;
                            let palette = app.settings.bodka.theme.palette();
                            let dim = app.settings.bodka.dim as f32 / 100.;
                            cx.notify();
                            Some((frost, palette, dim))
                        })
                        .unwrap();
                    if let Some((image, palette, dim)) = captured {
                        let shot = cx.update(|cx| {
                            let any: AnyWindowHandle = handle.into();
                            any.update(cx, |_, w, cx| {
                                w.refresh();
                                w.draw(cx).clear(cx);
                                w.render_to_image().expect("native GPU readback")
                            })
                            .unwrap()
                        });
                        let (width, height) = shot.dimensions();
                        assert!(
                            width >= 600 && height >= 400,
                            "native viewport was {width} × {height}"
                        );
                        let tex = image.size(0);
                        let (tw, th) = (u32::from(tex.width), u32::from(tex.height));
                        let scale = (width as f32 / tw as f32).max(height as f32 / th as f32);
                        let pixels = image.as_bytes(0).unwrap();
                        let tint = [16, 8, 0].map(|shift| ((palette.panel >> shift) & 255) as f32);
                        let mut matches = 0;
                        let mut sampled = 0;
                        for y in (height / 4..height * 4 / 5).step_by(7) {
                            for x in (width / 12..width * 2 / 3).step_by(7) {
                                let u = ((x as f32 - width as f32 / 2.) / scale + tw as f32 / 2.)
                                    as u32;
                                let v = ((y as f32 - height as f32 / 2.) / scale + th as f32 / 2.)
                                    as u32;
                                let pos = ((v.min(th - 1) * tw + u.min(tw - 1)) * 4) as usize;
                                let expected = [0, 1, 2].map(|n| {
                                    pixels[pos + 2 - n] as f32 * (1. - dim) * (1. - palette.opacity)
                                        + tint[n] * palette.opacity
                                });
                                let got = shot.get_pixel(x, y);
                                sampled += 1;
                                if expected[0] - expected[2] > 8.
                                    && (0..3).all(|n| (expected[n] - f32::from(got[n])).abs() < 14.)
                                {
                                    matches += 1;
                                }
                            }
                        }
                        shot.save(scratch.join("catalogue.png")).unwrap();
                        eprintln!("FROST_PIXELS matched={matches} sampled={sampled}");
                        handle
                            .update(cx, |app, _, cx| {
                                app.mods.browse.sort_menu = true;
                                cx.notify();
                            })
                            .unwrap();
                        let menu = cx.update(|cx| {
                            let any: AnyWindowHandle = handle.into();
                            any.update(cx, |_, w, cx| {
                                w.refresh();
                                w.draw(cx).clear(cx);
                                w.render_to_image().expect("native menu readback")
                            })
                            .unwrap()
                        });
                        let changed = shot
                            .pixels()
                            .zip(menu.pixels())
                            .filter(|(a, b)| a != b)
                            .count();
                        menu.save(scratch.join("catalogue-menu.png")).unwrap();
                        assert!(changed > 512, "the open sort menu must be visible");
                        *checked.lock().unwrap() = matches * 12 > sampled;
                        break;
                    }
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    assert!(*seen.lock().unwrap(), "the catalogue must paint the viewport-aligned frosted wallpaper after entering from a library");
}
