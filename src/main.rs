//! Excavation — entry point.
//!
//! Boots a 1280×720 window on desktop (or the WASM canvas in a browser), loads
//! and slices the asset sheets, loads `game.toml` + a map, and runs the
//! update/draw loop.
//!
//! ## Developer tool: `--editor [path]` (desktop only)
//!
//! Pass `--editor` (optionally followed by a map path) to open the M7 map editor
//! — a separate loop from the game that can create, edit, validate and save map
//! TOML files. It never runs on web (the flag is compiled out for wasm).
//!
//! ## Visual verification (desktop only)
//!
//! Pass `--screenshot <path>` to render a few frames and save a PNG of the game
//! to `<path>`, then exit. This is useful for headless/CI verification of what
//! the game actually draws (e.g. `cargo run -- --screenshot shot.png`). It is
//! desktop-only: screenshot export is not supported on web, and the argument is
//! compiled out for the wasm target. Coupled with `--editor`, it captures the
//! editor preview instead.

use macroquad::prelude::*;

mod app;
mod assets;
mod audio;
mod config;
#[cfg(not(target_arch = "wasm32"))]
mod editor;
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
    arg_after("--screenshot")
}

/// Parse the `--editor [path]` argument, returning the optional map path. The
/// editor (like `--screenshot`) is desktop-only: the flag is compiled out for the
/// wasm target.
#[cfg(not(target_arch = "wasm32"))]
fn editor_path() -> Option<String> {
    // `--editor` is an optional-value flag; only consume the following token as a
    // path when it does not itself look like another flag.
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--editor" {
            // Only consume the following token as a path when it does not itself
            // look like another flag.
            if args.get(i + 1).is_some_and(|n| !n.starts_with("--")) {
                return args.get(i + 1).cloned();
            }
            return Some(String::new());
        }
        i += 1;
    }
    None
}

/// Return the token immediately after `flag`, if present.
#[cfg(not(target_arch = "wasm32"))]
fn arg_after(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == flag {
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
    #[cfg(not(target_arch = "wasm32"))]
    let editor = editor_path();

    // Editor mode (desktop only): a separate, self-contained loop from `App`.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = editor {
        let assets = assets::Assets::load().await;
        let (cfg, load_err) = load_editor_config(path.as_str());
        let file_name = derive_file_name(path.as_str());
        // An empty `--editor` path means "no explicit save target", so the editor
        // falls back to `assets/maps/<file_name>.toml`.
        let base = if path.is_empty() { None } else { Some(path) };
        let mut editor = editor::Editor::new(cfg, &file_name, base, assets);
        if let Some(msg) = load_err {
            editor.set_status(msg);
        }
        run_editor_loop(&mut editor, screenshot).await;
        return;
    }

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
            if frame == 3
                && let Some(path) = &screenshot
            {
                let fb = render_target(screen_width() as u32, screen_height() as u32);
                app.render_to(&fb, screen_width() as u32, screen_height() as u32);
                fb.texture.get_texture_data().export_png(path);
                println!("screenshot saved to {path}");
                return;
            }
        }

        next_frame().await;
    }
}

/// Run the editor loop until the user quits (or a screenshot is captured).
#[cfg(not(target_arch = "wasm32"))]
async fn run_editor_loop(editor: &mut editor::Editor, screenshot: Option<String>) {
    let mut frame = 0u32;
    loop {
        let dt = get_frame_time();
        let quit = editor.update(dt);
        editor.draw();

        if let Some(path) = &screenshot {
            frame += 1;
            if frame == 3 {
                let fb = render_target(screen_width() as u32, screen_height() as u32);
                editor.render_to(&fb, screen_width() as u32, screen_height() as u32);
                fb.texture.get_texture_data().export_png(path);
                println!("screenshot saved to {path}");
                return;
            }
        }

        if quit {
            return;
        }
        next_frame().await;
    }
}

/// Load the config to edit: the file at `path` when it exists and parses, else a
/// fresh default map (so `--editor` always opens something editable). When the
/// file exists but is corrupt/unparseable, the error is returned so the editor can
/// surface it in its status rather than silently starting from scratch.
#[cfg(not(target_arch = "wasm32"))]
fn load_editor_config(path: &str) -> (config::map::MapConfig, Option<String>) {
    let mut err = None;
    let cfg = if !path.is_empty()
        && let Ok(text) = std::fs::read_to_string(path)
    {
        match config::map::MapConfig::from_toml(&text) {
            Ok(cfg) => cfg,
            Err(e) => {
                err = Some(format!("load error: {e}"));
                editor::EditorModel::default_config()
            }
        }
    } else {
        // Empty `--editor` path, or the file does not yet exist: start a fresh
        // default map bound to that path.
        editor::EditorModel::default_config()
    };
    (cfg, err)
}

/// Derive the editor's default file name from the path's file stem (`level01` for
/// `assets/maps/level01.toml`), or `new_map` when no path is given.
#[cfg(not(target_arch = "wasm32"))]
fn derive_file_name(path: &str) -> String {
    if path.is_empty() {
        return "new_map".to_string();
    }
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "new_map".to_string())
}
