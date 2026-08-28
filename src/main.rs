//! Excavation — M2 entry point.
//!
//! Boots a 1280×720 window on desktop (or the WASM canvas in a browser), loads
//! and slices the asset sheets, loads `game.toml` + a map, and runs the
//! update/draw loop.
//!
//! ## Visual verification (desktop only)
//!
//! Pass `--screenshot <path>` to render a few frames and save a PNG of the game
//! to `<path>`, then exit. This is useful for headless/CI verification of what
//! the game actually draws (e.g. `cargo run -- --screenshot shot.png`). It is
//! desktop-only: screenshot export is not supported on web, and the argument is
//! compiled out for the wasm target.

use macroquad::prelude::*;

mod app;
mod assets;
mod audio;
mod config;
mod game;
mod hud;
mod input;
mod menu;
mod save;
mod settings;
mod ui;

fn window_conf() -> Conf {
    Conf {
        window_title: "Excavation".to_owned(),
        window_width: 1280,
        window_height: 720,
        ..Default::default()
    }
}

/// Parse the `--screenshot <path>` argument, returning the output path if set.
#[cfg(not(target_arch = "wasm32"))]
fn screenshot_path() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" {
            return args.get(i + 1).cloned();
        }
        i += 1;
    }
    None
}

#[macroquad::main(window_conf)]
async fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let screenshot = screenshot_path();

    let mut app = app::App::new().await;

    #[cfg(not(target_arch = "wasm32"))]
    let mut frame = 0u32;

    loop {
        let dt = get_frame_time();
        app.update(dt);
        app.draw();

        // Capture a frame for visual verification, then exit (desktop only).
        // We render the scene into a RenderTarget (not the live screen) so the
        // capture is reliable, then read the target's texture back and save it
        // as PNG.
        #[cfg(not(target_arch = "wasm32"))]
        {
            frame += 1;
            if let Some(path) = &screenshot {
                if frame == 3 {
                    let fb = render_target(screen_width() as u32, screen_height() as u32);
                    app.render_to(&fb, screen_width() as u32, screen_height() as u32);
                    fb.texture.get_texture_data().export_png(path);
                    println!("screenshot saved to {path}");
                    return;
                }
            }
        }

        next_frame().await;
    }
}
