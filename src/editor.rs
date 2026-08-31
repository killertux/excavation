//! M7 map editor: a desktop-only developer tool (`--editor [path]`) to create,
//! edit, validate and save map TOML files that the game then loads.
//!
//! ## Structure
//!
//! The work is split the same way as the game (pure sim + thin view):
//!
//! - [`EditorModel`] holds the configuration being edited plus the cursor/tool/
//!   mode/field selection. All the **rules** live here as pure methods — placing a
//!   gap on the border only, toggling a structure on an interior cell only,
//!   clamping dimensions, validating-on-save, filename normalization. Everything
//!   here is unit-testable without a window or GPU.
//! - [`Editor`] wraps [`EditorModel`] with the [`Assets`] terrain sprites and
//!   draws the grid preview + field/status panel, and polls keyboard input.
//!
//! ## Modes
//!
//! - `Grid` — move a cursor and apply the active tool (start/exit/structure).
//! - `Fields` — select one of the 8 numeric config rows and nudge its value.
//! - `Filename` — type/backspace the save filename (an extension the plan notes
//!   as "a minimal text field"; it needs its own mode so `s`/`l` can be typed
//!   without triggering save/load).
//!
//! ## Save / Load
//!
//! Saving validates first and **never writes** a bad file. The path is the
//! `--editor <path>` argument when given, else `assets/maps/<file_name>.toml`.

use macroquad::prelude::*;

use crate::assets::Assets;
use crate::config::ConfigError;
use crate::config::map::{MapConfig, Pos};
use crate::game::TILE_SIZE;
use crate::game::map::Tile;
use crate::game::terrain;

/// Smallest map dimension the editor allows (so `width - 2` interior is non-empty).
const MIN_DIM: usize = 5;
/// Largest map dimension the editor allows (keeps the preview tile readable).
const MAX_DIM: usize = 100;
/// Increment applied by a single Left/Right on a multiplier field.
const MULT_STEP: f32 = 0.1;
/// Upper bound for multiplier fields (and a positive floor, so beasts never get a
/// zero speed/mining time which would be degenerate).
const MAX_MULT: f32 = 10.0;
const MIN_MULT: f32 = 0.1;

/// Editor background (matching the game's dark clear colour).
const BG: Color = Color::new(24.0 / 255.0, 24.0 / 255.0, 34.0 / 255.0, 1.0);

/// The active grid placement tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Start,
    Exit,
    Structure,
}

/// Which column of the editor the user is working in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Grid,
    Fields,
    Filename,
}

/// A configurable numeric row in Fields mode (in display order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Width,
    Height,
    Unmineable,
    Gold,
    Beast,
    Speed,
    Mining,
    Seed,
}

impl Field {
    /// All fields, in display (and `field_index`) order.
    pub const ALL: [Field; 8] = [
        Field::Width,
        Field::Height,
        Field::Unmineable,
        Field::Gold,
        Field::Beast,
        Field::Speed,
        Field::Mining,
        Field::Seed,
    ];
}

/// Whether `(x, y)` sits on the map's border ring.
fn is_border(x: i32, y: i32, w: usize, h: usize) -> bool {
    x == 0 || y == 0 || x == (w as i32 - 1) || y == (h as i32 - 1)
}

/// Normalize a file name to end in `.toml` (the editor never writes a
/// non-TOML map file). An empty name falls back to `new_map.toml` so a
/// fully-backspaced filename can never produce a bare `.toml` path.
pub fn normalize_filename(name: &str) -> String {
    if name.is_empty() {
        return "new_map.toml".to_string();
    }
    if name.ends_with(".toml") {
        name.to_string()
    } else {
        format!("{name}.toml")
    }
}

/// A screen-space camera that targets an optional render target (for the
/// `--screenshot` readback), mirroring `App`'s proven convention.
fn set_screen_camera(rt: Option<RenderTarget>, view_w: f32, view_h: f32) {
    if let Some(rt) = rt {
        set_camera(&Camera2D {
            target: Vec2::new(view_w / 2.0, view_h / 2.0),
            zoom: Vec2::new(2.0 / view_w, -2.0 / view_h),
            render_target: Some(rt),
            ..Default::default()
        });
    } else {
        set_default_camera();
    }
}

/// Project an arbitrary cell onto the nearest border-ring cell (so a gap stays
/// a valid gap after a resize).
fn clamp_border(pos: Pos, w: usize, h: usize) -> Pos {
    let bw = (w as i32) - 1;
    let bh = (h as i32) - 1;
    let x = pos.x.clamp(0, bw);
    let y = pos.y.clamp(0, bh);
    let dxl = x;
    let dxr = bw - x;
    let dyt = y;
    let dyb = bh - y;
    let dx = dxl.min(dxr);
    let dy = dyt.min(dyb);
    if dx < dy {
        if dxl < dxr {
            Pos { x: 0, y }
        } else {
            Pos { x: bw, y }
        }
    } else if dy < dx {
        if dyt < dyb {
            Pos { x, y: 0 }
        } else {
            Pos { x, y: bh }
        }
    } else {
        // Tie: snap to the top edge for determinism (still on the ring).
        Pos { x, y: 0 }
    }
}

/// The first border-ring cell that is not `after` (used to keep the two gaps
/// distinct after a resize).
fn next_border(after: Pos, w: usize, h: usize) -> Pos {
    let bw = (w as i32) - 1;
    let bh = (h as i32) - 1;
    for y in 0..=bh {
        for x in 0..=bw {
            let p = Pos { x, y };
            if (x == 0 || y == 0 || x == bw || y == bh) && p != after {
                return p;
            }
        }
    }
    after
}

/// The pure, testable editor core: config + cursor + tool/mode/field selection.
#[derive(Debug, Clone)]
pub struct EditorModel {
    cfg: MapConfig,
    cursor: (i32, i32),
    tool: Tool,
    mode: Mode,
    field_index: usize,
    file_name: String,
    status: Option<String>,
}

impl EditorModel {
    /// A sensible default map to start a brand-new edit session with.
    pub fn default_config() -> MapConfig {
        MapConfig {
            width: 30,
            height: 20,
            unmineable_count: 20,
            gold_count: 8,
            beast_count: 1,
            beast_speed_multiplier: 1.0,
            beast_mining_time_multiplier: 1.0,
            start: Pos { x: 15, y: 19 },
            exit: Pos { x: 5, y: 0 },
            seed: None,
            structures: Vec::new(),
        }
    }

    /// Create a model editing `cfg`, with the cursor parked on the start gap.
    pub fn new(cfg: MapConfig, file_name: &str) -> EditorModel {
        let cursor = clamp_cursor(&cfg, (cfg.start.x, cfg.start.y));
        EditorModel {
            cfg,
            cursor,
            tool: Tool::Structure,
            mode: Mode::Grid,
            field_index: 0,
            file_name: file_name.to_string(),
            status: None,
        }
    }

    // ---- Accessors (for the renderer / top-level) ------------------------

    pub fn cfg(&self) -> &MapConfig {
        &self.cfg
    }
    pub fn cursor(&self) -> (i32, i32) {
        self.cursor
    }
    pub fn tool(&self) -> Tool {
        self.tool
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn field_index(&self) -> usize {
        self.field_index
    }
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    // ---- Grid tooling ----------------------------------------------------

    /// Move the cursor by a grid delta, clamped to the map.
    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        self.cursor = clamp_cursor(&self.cfg, (self.cursor.0 + dx, self.cursor.1 + dy));
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// Apply the active tool at the cursor.
    pub fn apply_tool(&mut self) {
        let (x, y) = self.cursor;
        match self.tool {
            Tool::Start => self.place_gap(true, x, y),
            Tool::Exit => self.place_gap(false, x, y),
            Tool::Structure => self.toggle_structure(x, y),
        }
    }

    /// Place (or move) one of the two border gaps at `(x, y)`, which must be on
    /// the border and distinct from the other gap.
    fn place_gap(&mut self, start: bool, x: i32, y: i32) {
        if !is_border(x, y, self.cfg.width, self.cfg.height) {
            return;
        }
        let target = Pos { x, y };
        let other = if start { self.cfg.exit } else { self.cfg.start };
        if target == other {
            return;
        }
        if start {
            self.cfg.start = target;
        } else {
            self.cfg.exit = target;
        }
    }

    /// Toggle an unbreakable structure on an **interior** cell (never the border
    /// ring, the start gap, or the exit gap).
    fn toggle_structure(&mut self, x: i32, y: i32) {
        if is_border(x, y, self.cfg.width, self.cfg.height) {
            return;
        }
        let start = (self.cfg.start.x, self.cfg.start.y);
        let exit = (self.cfg.exit.x, self.cfg.exit.y);
        let cell = [x, y];
        if cell == [start.0, start.1] || cell == [exit.0, exit.1] {
            return;
        }
        if let Some(i) = self.cfg.structures.iter().position(|&s| s == cell) {
            self.cfg.structures.remove(i);
        } else {
            self.cfg.structures.push(cell);
        }
    }

    // ---- Field editing ---------------------------------------------------

    /// The field currently selected in Fields mode.
    pub fn current_field(&self) -> Field {
        match self.field_index {
            0 => Field::Width,
            1 => Field::Height,
            2 => Field::Unmineable,
            3 => Field::Gold,
            4 => Field::Beast,
            5 => Field::Speed,
            6 => Field::Mining,
            _ => Field::Seed,
        }
    }

    /// Move the Fields-mode selection up (`delta < 0`) or down (`delta > 0`).
    pub fn move_field_selection(&mut self, delta: i32) {
        let idx = self.field_index as i32 + delta;
        self.field_index = idx.clamp(0, Field::ALL.len() as i32 - 1) as usize;
    }

    /// Nudge the selected numeric field by `delta` (usually +1/-1), applying each
    /// field's clamp. Width/Height route through [`EditorModel::set_dimensions`]
    /// so start/exit/structures are re-clamped as the grid changes.
    pub fn adjust_field(&mut self, delta: i32) {
        match self.current_field() {
            Field::Width => {
                let nw = (self.cfg.width as i64 + delta as i64)
                    .clamp(MIN_DIM as i64, MAX_DIM as i64) as usize;
                self.set_dimensions(nw, self.cfg.height);
            }
            Field::Height => {
                let nh = (self.cfg.height as i64 + delta as i64)
                    .clamp(MIN_DIM as i64, MAX_DIM as i64) as usize;
                self.set_dimensions(self.cfg.width, nh);
            }
            Field::Unmineable => {
                self.cfg.unmineable_count =
                    (self.cfg.unmineable_count as i64 + delta as i64).max(0) as usize;
            }
            Field::Gold => {
                self.cfg.gold_count = (self.cfg.gold_count as i64 + delta as i64).max(0) as u32;
            }
            Field::Beast => {
                self.cfg.beast_count = (self.cfg.beast_count as i64 + delta as i64).max(0) as u32;
            }
            Field::Speed => {
                self.cfg.beast_speed_multiplier = (self.cfg.beast_speed_multiplier
                    + delta as f32 * MULT_STEP)
                    .clamp(MIN_MULT, MAX_MULT);
            }
            Field::Mining => {
                self.cfg.beast_mining_time_multiplier = (self.cfg.beast_mining_time_multiplier
                    + delta as f32 * MULT_STEP)
                    .clamp(MIN_MULT, MAX_MULT);
            }
            Field::Seed => {
                let cur = self.cfg.seed.unwrap_or(0);
                let nv = (cur as i64 + delta as i64).max(0) as u64;
                self.cfg.seed = Some(nv);
            }
        }
    }

    /// Reset the seed to "random" (`None`).
    pub fn clear_seed(&mut self) {
        self.cfg.seed = None;
    }

    /// Resize the map, clamping start/exit onto the border (and keeping them
    /// distinct) and clamping/dropping structures so they stay interior.
    pub fn set_dimensions(&mut self, w: usize, h: usize) {
        let nw = w.clamp(MIN_DIM, MAX_DIM);
        let nh = h.clamp(MIN_DIM, MAX_DIM);
        let start = clamp_border(self.cfg.start, nw, nh);
        let mut exit = clamp_border(self.cfg.exit, nw, nh);
        if start == exit {
            exit = next_border(start, nw, nh);
        }
        let start_cell = [start.x, start.y];
        let exit_cell = [exit.x, exit.y];
        let mut seen = std::collections::HashSet::new();
        self.cfg.structures.retain_mut(|s| {
            let ci = (nw as i32) - 2;
            let cj = (nh as i32) - 2;
            s[0] = s[0].clamp(1, ci);
            s[1] = s[1].clamp(1, cj);
            let cell = *s;
            (cell != start_cell && cell != exit_cell) && seen.insert(cell)
        });
        self.cfg.start = start;
        self.cfg.exit = exit;
        self.cfg.width = nw;
        self.cfg.height = nh;
        self.cursor = clamp_cursor(&self.cfg, self.cursor);
    }

    // ---- Filename typing -------------------------------------------------

    /// Append a character to the filename, accepting only basic filename
    /// characters (no path separators, no dots — the `.toml` extension is added
    /// on save).
    pub fn type_filename_char(&mut self, c: char) {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            self.file_name.push(c);
        }
    }

    /// Remove the last filename character.
    pub fn backspace_filename(&mut self) {
        self.file_name.pop();
    }

    /// The normalized save filename (always `.toml`).
    pub fn filename(&self) -> String {
        normalize_filename(&self.file_name)
    }

    // ---- Save / Load -----------------------------------------------------

    /// Resolve the file to save/load: the explicit `--editor` path if given, else
    /// the default `assets/maps/<file_name>.toml`.
    pub fn save_path(&self, base: Option<&str>) -> String {
        match base {
            Some(p) => normalize_filename(p),
            None => format!("assets/maps/{}", self.filename()),
        }
    }

    /// Validate the whole config.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.cfg.validate()
    }

    /// Serialize to TOML after validating (never serializes an invalid map).
    /// This is the save-time guard: if it errs, the editor must not write.
    pub fn serialize_checked(&self) -> Result<String, ConfigError> {
        self.validate()?;
        self.to_toml()
    }

    /// Serialize without validating (used for round-trip checks).
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        self.cfg.to_toml()
    }

    /// Replace the config from a TOML document, re-clamping the cursor.
    pub fn load_toml(&mut self, text: &str) -> Result<(), ConfigError> {
        let cfg = MapConfig::from_toml(text)?;
        self.cfg = cfg;
        self.cursor = clamp_cursor(&self.cfg, (self.cfg.start.x, self.cfg.start.y));
        self.field_index = 0;
        Ok(())
    }

    /// Validate-then-write to `path`. On an invalid config, sets `status` and
    /// returns `Err` **without** writing. On success, sets `status` and returns
    /// `Ok`. Returns the path actually written.
    pub fn save_to(&mut self, path: &str) -> Result<String, String> {
        let path = normalize_filename(path);
        match self.serialize_checked() {
            Ok(text) => {
                std::fs::write(&path, text).map_err(|e| {
                    let msg = format!("write {path}: {e}");
                    self.status = Some(msg.clone());
                    msg
                })?;
                self.status = Some(format!("saved {path}"));
                Ok(path)
            }
            Err(e) => {
                let msg = format!("invalid map — not saved: {e}");
                self.status = Some(msg.clone());
                Err(msg)
            }
        }
    }

    /// Read a TOML file and replace the config. On any error, sets `status`.
    pub fn load_from(&mut self, path: &str) -> Result<(), String> {
        let path = normalize_filename(path);
        match std::fs::read_to_string(&path) {
            Ok(text) => self.load_toml(&text).map_err(|e| {
                let msg = format!("load error: {e}");
                self.status = Some(msg.clone());
                msg
            }),
            Err(e) => {
                let msg = format!("read error: {e}");
                self.status = Some(msg.clone());
                Err(msg)
            }
        }
    }

    // ---- Preview ---------------------------------------------------------

    /// The tile to draw in the grid preview for `(x, y)`: start/exit gaps as
    /// dirt, the border ring + structures as unbreakable, everything else as
    /// mineable. Out-of-bounds reads as unbreakable (like the game).
    pub fn preview_tile(&self, x: i32, y: i32) -> Tile {
        let w = self.cfg.width as i32;
        let h = self.cfg.height as i32;
        if x < 0 || y < 0 || x >= w || y >= h {
            return Tile::Unbreakable;
        }
        let cell = [x, y];
        if (x, y) == (self.cfg.start.x, self.cfg.start.y)
            || (x, y) == (self.cfg.exit.x, self.cfg.exit.y)
        {
            return Tile::Dirt;
        }
        if is_border(x, y, self.cfg.width, self.cfg.height) {
            return Tile::Unbreakable;
        }
        if self.cfg.structures.contains(&cell) {
            return Tile::Unbreakable;
        }
        Tile::Mineable
    }
}

/// Clamp a cursor coordinate into the current grid.
fn clamp_cursor(cfg: &MapConfig, (x, y): (i32, i32)) -> (i32, i32) {
    let w = cfg.width as i32;
    let h = cfg.height as i32;
    (
        x.clamp(0, w.saturating_sub(1)),
        y.clamp(0, h.saturating_sub(1)),
    )
}

/// The rendering wrapper: model + terrain sprites + the `--editor` base path.
pub struct Editor {
    model: EditorModel,
    assets: Assets,
    base_path: Option<String>,
}

impl Editor {
    /// Build the editor. `base_path` is the `--editor <path>` argument (if any);
    /// it becomes the save/load target, else `assets/maps/<file_name>.toml`.
    pub fn new(
        cfg: MapConfig,
        file_name: &str,
        base_path: Option<String>,
        assets: Assets,
    ) -> Editor {
        Editor {
            model: EditorModel::new(cfg, file_name),
            assets,
            base_path,
        }
    }

    /// Advance one frame, returning `true` when the user quits.
    pub fn update(&mut self, _dt: f32) -> bool {
        match self.model.mode {
            Mode::Grid => self.update_grid(),
            Mode::Fields => self.update_fields(),
            Mode::Filename => self.update_filename(),
        }
    }

    /// Set a transient status message (e.g. a startup load error surfaced before
    /// the first frame, since `--editor <path>` loads before the loop begins).
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.model.status = Some(msg.into());
    }

    fn update_grid(&mut self) -> bool {
        if is_key_pressed(KeyCode::Escape) {
            return true;
        }
        if is_key_pressed(KeyCode::Up) {
            self.model.move_cursor(0, -1);
        }
        if is_key_pressed(KeyCode::Down) {
            self.model.move_cursor(0, 1);
        }
        if is_key_pressed(KeyCode::Left) {
            self.model.move_cursor(-1, 0);
        }
        if is_key_pressed(KeyCode::Right) {
            self.model.move_cursor(1, 0);
        }
        if is_key_pressed(KeyCode::Key1) {
            self.model.set_tool(Tool::Start);
        }
        if is_key_pressed(KeyCode::Key2) {
            self.model.set_tool(Tool::Exit);
        }
        if is_key_pressed(KeyCode::Key3) {
            self.model.set_tool(Tool::Structure);
        }
        if is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::Enter) {
            self.model.apply_tool();
        }
        if is_key_pressed(KeyCode::Tab) {
            self.model.set_mode(Mode::Fields);
        }
        if is_key_pressed(KeyCode::S) {
            self.save();
        }
        if is_key_pressed(KeyCode::L) {
            self.load();
        }
        if is_key_pressed(KeyCode::F) {
            // Drop any queued chars (e.g. the 'f' that triggered this) so they are
            // not appended to the filename.
            while get_char_pressed().is_some() {}
            self.model.set_mode(Mode::Filename);
        }
        false
    }

    fn update_fields(&mut self) -> bool {
        if is_key_pressed(KeyCode::Escape) {
            return true;
        }
        if is_key_pressed(KeyCode::Up) {
            self.model.move_field_selection(-1);
        }
        if is_key_pressed(KeyCode::Down) {
            self.model.move_field_selection(1);
        }
        if is_key_pressed(KeyCode::Left) {
            self.model.adjust_field(-1);
        }
        if is_key_pressed(KeyCode::Right) {
            self.model.adjust_field(1);
        }
        if is_key_pressed(KeyCode::Backspace) && self.model.current_field() == Field::Seed {
            self.model.clear_seed();
        }
        if is_key_pressed(KeyCode::Tab) {
            self.model.set_mode(Mode::Grid);
        }
        if is_key_pressed(KeyCode::S) {
            self.save();
        }
        if is_key_pressed(KeyCode::L) {
            self.load();
        }
        if is_key_pressed(KeyCode::F) {
            // See `update_grid`: drop queued chars before editing the filename.
            while get_char_pressed().is_some() {}
            self.model.set_mode(Mode::Filename);
        }
        false
    }

    fn update_filename(&mut self) -> bool {
        if is_key_pressed(KeyCode::Escape)
            || is_key_pressed(KeyCode::Tab)
            || is_key_pressed(KeyCode::Enter)
        {
            self.model.set_mode(Mode::Grid);
            return false;
        }
        if is_key_pressed(KeyCode::Backspace) {
            self.model.backspace_filename();
            return false;
        }
        while let Some(c) = get_char_pressed() {
            self.model.type_filename_char(c);
        }
        false
    }

    fn save(&mut self) {
        let path = self.model.save_path(self.base_path.as_deref());
        let _ = self.model.save_to(&path);
    }

    fn load(&mut self) {
        let path = self.model.save_path(self.base_path.as_deref());
        let _ = self.model.load_from(&path);
    }

    // ---- Rendering -------------------------------------------------------

    /// Draw the current editor frame to the (live) screen.
    pub fn draw(&self) {
        self.draw_frame(screen_width(), screen_height(), None);
    }

    /// Render one frame into `fb` for the `--screenshot` flag (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_to(&self, fb: &RenderTarget, w: u32, h: u32) {
        self.draw_frame(w as f32, h as f32, Some(fb.clone()));
    }

    fn draw_frame(&self, view_w: f32, view_h: f32, rt: Option<RenderTarget>) {
        set_screen_camera(rt.clone(), view_w, view_h);
        clear_background(BG);

        let cell = self.grid_pixel_size(view_w, view_h);
        let grid_w_px = self.model.cfg.width as f32 * cell;
        let grid_h_px = self.model.cfg.height as f32 * cell;
        // Put the grid in the left ~58% of the window, centered vertically.
        let grid_x = (view_w * 0.58 - grid_w_px) / 2.0;
        let grid_y = (view_h - grid_h_px) / 2.0;
        self.draw_grid(grid_x, grid_y, cell);

        // Right-hand panel.
        let panel_x = view_w * 0.60;
        self.draw_panel(panel_x, 28.0, view_w, view_h);

        // Reset the camera so the (possibly render-target) pass is flushed before
        // the caller reads the texture back (e.g. `--screenshot`), matching
        // `App`'s menu/draw paths.
        set_default_camera();
    }

    /// Tile size (px) so the whole grid snugly fits the left region, capped so a
    /// large map de-scaled small still renders vs. a small map not ballooning.
    fn grid_pixel_size(&self, view_w: f32, view_h: f32) -> f32 {
        let region_w = view_w * 0.58;
        let region_h = view_h * 0.9;
        let gw = self.model.cfg.width as f32;
        let gh = self.model.cfg.height as f32;
        let scale = (region_w / (gw * TILE_SIZE))
            .min(region_h / (gh * TILE_SIZE))
            .min(1.5);
        (TILE_SIZE * scale).max(4.0)
    }

    fn draw_grid(&self, ox: f32, oy: f32, cell: f32) {
        let cfg = self.model.cfg();
        for y in 0..cfg.height {
            for x in 0..cfg.width {
                let xi = x as i32;
                let yi = y as i32;
                let center = self.model.preview_tile(xi, yi);
                let n = self.model.preview_tile(xi, yi - 1);
                let e = self.model.preview_tile(xi + 1, yi);
                let s = self.model.preview_tile(xi, yi + 1);
                let w = self.model.preview_tile(xi - 1, yi);
                let sel = terrain::tile_atlas(center, n, e, s, w);
                let px = ox + x as f32 * cell;
                let py = oy + y as f32 * cell;
                if let Some(under) = terrain::underlay(center, n, e, s, w) {
                    self.draw_tex(self.assets.tile(under), px, py, cell);
                }
                self.draw_tex(self.assets.tile(sel), px, py, cell);
            }
        }

        // Tint the start (green) and exit (red) gaps.
        let sx = cfg.start.x as f32;
        let sy = cfg.start.y as f32;
        let ex = cfg.exit.x as f32;
        let ey = cfg.exit.y as f32;
        draw_rectangle(
            ox + sx * cell,
            oy + sy * cell,
            cell,
            cell,
            Color::new(0.2, 1.0, 0.2, 0.45),
        );
        draw_rectangle(
            ox + ex * cell,
            oy + ey * cell,
            cell,
            cell,
            Color::new(1.0, 0.3, 0.3, 0.45),
        );

        // Cursor highlight.
        let (cx, cy) = self.model.cursor();
        draw_rectangle(
            ox + cx as f32 * cell + 1.0,
            oy + cy as f32 * cell + 1.0,
            cell - 2.0,
            cell - 2.0,
            Color::new(1.0, 1.0, 0.2, 0.5),
        );

        // Border ring outline.
        draw_rectangle_lines(
            ox - 1.0,
            oy - 1.0,
            cfg.width as f32 * cell + 2.0,
            cfg.height as f32 * cell + 2.0,
            2.0,
            WHITE,
        );
    }

    fn draw_tex(&self, tex: &Texture2D, x: f32, y: f32, size: f32) {
        draw_texture_ex(
            tex,
            x,
            y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(size, size)),
                ..Default::default()
            },
        );
    }

    fn draw_panel(&self, x: f32, start_y: f32, view_w: f32, view_h: f32) {
        let line_h = 20.0;
        let mut y = start_y;
        draw_text("MAP EDITOR", x, y, 28.0, WHITE);
        y += 40.0;

        y = self.info_line(x, y, "Tool", tool_name(self.model.tool()));
        y = self.info_line(x, y, "Mode", mode_name(self.model.mode()));
        y += 8.0;

        // The 8 numeric fields.
        let selected = if self.model.mode() == Mode::Fields {
            Some(self.model.field_index())
        } else {
            None
        };
        for (i, field) in Field::ALL.iter().enumerate() {
            let color = if selected == Some(i) { YELLOW } else { WHITE };
            draw_text(field_label(*field), x, y, 14.0, color);
            draw_text(self.field_value(*field), x + 130.0, y, 14.0, color);
            y += line_h;
        }
        y += 10.0;

        y = self.info_line(x, y, "File", &self.model.filename());
        y = self.info_line(
            x,
            y,
            "Path",
            &self.model.save_path(self.base_path.as_deref()),
        );
        y += 12.0;

        match self.model.status() {
            Some(status) => draw_text(status, x, y, 13.0, GREEN),
            None => draw_text("ready", x, y, 13.0, GRAY),
        };

        // Footer key hints.
        let hints = self.footer_hints();
        draw_text(&hints, x, view_h - 26.0, 12.0, GRAY);
        let _ = view_w;
    }

    /// Draw a `label`/`value` line, returning the next line's y.
    fn info_line(&self, x: f32, y: f32, label: &str, value: &str) -> f32 {
        draw_text(label, x, y, 13.0, GRAY);
        draw_text(value, x + 90.0, y, 13.0, WHITE);
        y + 20.0
    }

    fn field_value(&self, field: Field) -> String {
        let cfg = self.model.cfg();
        match field {
            Field::Width => cfg.width.to_string(),
            Field::Height => cfg.height.to_string(),
            Field::Unmineable => cfg.unmineable_count.to_string(),
            Field::Gold => cfg.gold_count.to_string(),
            Field::Beast => cfg.beast_count.to_string(),
            Field::Speed => format!("{:.1}", cfg.beast_speed_multiplier),
            Field::Mining => format!("{:.1}", cfg.beast_mining_time_multiplier),
            Field::Seed => cfg
                .seed
                .map(|s| s.to_string())
                .unwrap_or_else(|| "random".to_string()),
        }
    }

    fn footer_hints(&self) -> String {
        match self.model.mode() {
            Mode::Grid => "Grid: arrows=cursor 1/2/3=tool Space=place Tab=fields S=save L=load F=file Esc=quit".to_string(),
            Mode::Fields => "Fields: up/down=field left/right=value Backspace=random-seed Tab=grid S=save L=load F=file Esc=quit".to_string(),
            Mode::Filename => "Filename: type=chars Backspace=delete Enter=done Esc=back".to_string(),
        }
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Start => "start",
        Tool::Exit => "exit",
        Tool::Structure => "structure",
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Grid => "grid",
        Mode::Fields => "fields",
        Mode::Filename => "filename",
    }
}

fn field_label(field: Field) -> &'static str {
    match field {
        Field::Width => "width",
        Field::Height => "height",
        Field::Unmineable => "unmineable",
        Field::Gold => "gold",
        Field::Beast => "beast",
        Field::Speed => "beast speed x",
        Field::Mining => "beast mine x",
        Field::Seed => "seed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small valid map for assertions.
    fn base_model() -> EditorModel {
        EditorModel::new(EditorModel::default_config(), "test")
    }

    #[test]
    fn cursor_stays_in_bounds() {
        let mut m = base_model();
        // Start cursor is on the start gap (15,19); push far beyond each corner.
        m.move_cursor(1000, 1000);
        assert_eq!(m.cursor(), (29, 19));
        m.move_cursor(-1000, -1000);
        assert_eq!(m.cursor(), (0, 0));
    }

    #[test]
    fn start_and_exit_only_place_on_the_border() {
        let mut m = base_model();
        m.set_tool(Tool::Start);
        // Move to a known interior cell (y=10 is strictly inside a 20-tall map).
        let interior = (10, 10);
        m.move_cursor(interior.0 - m.cursor().0, interior.1 - m.cursor().1);
        assert!(
            !is_border(m.cursor().0, m.cursor().1, 30, 20),
            "cursor should be interior"
        );
        let cfg_before = m.cfg().start;
        m.apply_tool();
        assert_eq!(
            m.cfg().start,
            cfg_before,
            "start must not move onto an interior cell"
        );
    }

    #[test]
    fn start_and_exit_cannot_overlap() {
        let mut m = base_model();
        // Move start onto the exit gap's cell.
        m.set_tool(Tool::Start);
        let exit = m.cfg().exit;
        m.move_cursor(exit.x - m.cursor().0, exit.y - m.cursor().1);
        m.apply_tool();
        // Placing start at the exit cell must be a no-op (start unchanged).
        assert_eq!(m.cfg().start, Pos { x: 15, y: 19 });
    }

    #[test]
    fn structure_toggles_add_and_remove_on_interior_only() {
        let mut m = base_model();
        // Place on an interior cell.
        m.set_tool(Tool::Structure);
        let mut cursor = m.cursor();
        m.move_cursor(10 - cursor.0, 10 - cursor.1);
        cursor = m.cursor();
        m.apply_tool();
        assert!(m.cfg().structures.contains(&[cursor.0, cursor.1]));

        // Toggling again removes it.
        m.apply_tool();
        assert!(!m.cfg().structures.contains(&[cursor.0, cursor.1]));
    }

    #[test]
    fn structure_ignores_border_start_and_exit() {
        let mut m = base_model();
        m.set_tool(Tool::Structure);
        // Try to place on the border ring (0,0). Must be ignored.
        m.move_cursor(-100, -100);
        m.apply_tool();
        assert!(
            m.cfg().structures.is_empty(),
            "border placement must be ignored"
        );

        // Try to place on the exit gap. Must be ignored.
        let exit = m.cfg().exit;
        let cursor = m.cursor();
        m.move_cursor(exit.x - cursor.0, exit.y - cursor.1);
        m.apply_tool();
        assert!(
            m.cfg().structures.is_empty(),
            "start/exit placement must be ignored"
        );
    }

    #[test]
    fn set_dimensions_clamps_entities_into_new_bounds() {
        // Start from the default 30x20 with a couple of interior structures.
        let mut m = base_model();
        m.set_tool(Tool::Structure);
        for (sx, sy) in [(3, 3), (4, 4)] {
            let cursor = m.cursor();
            m.move_cursor(sx - cursor.0, sy - cursor.1);
            m.apply_tool();
        }
        assert_eq!(m.cfg().structures.len(), 2);

        m.set_dimensions(8, 8);
        let cfg = m.cfg();
        assert_eq!(cfg.width, 8);
        assert_eq!(cfg.height, 8);
        // start/exit on the border and distinct.
        assert!(is_border(cfg.start.x, cfg.start.y, 8, 8));
        assert!(is_border(cfg.exit.x, cfg.exit.y, 8, 8));
        assert_ne!(cfg.start, cfg.exit);
        // All structures interior (1..6 on both axes).
        for s in &cfg.structures {
            assert!(
                s[0] >= 1 && s[0] <= 6 && s[1] >= 1 && s[1] <= 6,
                "structure {:?} must be interior",
                s
            );
        }
        // The resized map must still be a valid config.
        cfg.validate().expect("resized config is valid");
    }

    #[test]
    fn adjust_field_clamps_counts_and_dims() {
        let mut m = base_model();
        m.set_mode(Mode::Fields);

        // Width: from 30, floor at 5.
        m.field_index = 0;
        for _ in 0..100 {
            m.adjust_field(-1);
        }
        assert_eq!(m.cfg().width, MIN_DIM);
        assert_eq!(m.cfg().height, 20, "height untouched");

        // unmineable_count can't go negative.
        m.field_index = Field::ALL
            .iter()
            .position(|f| *f == Field::Unmineable)
            .unwrap();
        for _ in 0..100 {
            m.adjust_field(-1);
        }
        assert_eq!(m.cfg().unmineable_count, 0);
    }

    #[test]
    fn adjust_field_clamps_multipliers_positive() {
        let mut m = base_model();
        m.set_mode(Mode::Fields);
        m.field_index = 5; // Speed
        for _ in 0..100 {
            m.adjust_field(-1);
        }
        let v = m.cfg().beast_speed_multiplier;
        assert!((v - MIN_MULT).abs() < 1e-4, "must not hit zero, got {v}");
        m.field_index = 6; // Mining
        for _ in 0..100 {
            m.adjust_field(-1);
        }
        let w = m.cfg().beast_mining_time_multiplier;
        assert!((w - MIN_MULT).abs() < 1e-4, "must not hit zero, got {w}");
    }

    #[test]
    fn seed_field_sets_value_and_backspace_clears() {
        let mut m = base_model();
        m.set_mode(Mode::Fields);
        m.field_index = 7; // Seed
        assert_eq!(m.cfg().seed, None);
        m.adjust_field(1);
        assert_eq!(m.cfg().seed, Some(1));
        m.adjust_field(9);
        assert_eq!(m.cfg().seed, Some(10));
        m.clear_seed();
        assert_eq!(m.cfg().seed, None);
    }

    #[test]
    fn move_field_selection_stays_in_range() {
        let mut m = base_model();
        m.set_mode(Mode::Fields);
        for _ in 0..50 {
            m.move_field_selection(1);
        }
        assert_eq!(m.field_index(), Field::ALL.len() - 1);
        for _ in 0..50 {
            m.move_field_selection(-1);
        }
        assert_eq!(m.field_index(), 0);
    }

    #[test]
    fn validate_rejects_off_border_gaps_and_over_capacity() {
        let mut m = base_model();
        m.cfg.exit = Pos { x: 5, y: 4 }; // interior
        assert!(matches!(m.validate(), Err(ConfigError::Validation(_))));

        let mut m2 = base_model();
        m2.cfg.unmineable_count = 100_000;
        assert!(matches!(m2.validate(), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn serialize_checked_rejects_invalid_config() {
        let mut m = base_model();
        m.cfg.exit = Pos { x: 5, y: 4 };
        assert!(matches!(
            m.serialize_checked(),
            Err(ConfigError::Validation(_))
        ));
    }

    #[test]
    fn filename_normalization_appends_toml() {
        assert_eq!(normalize_filename("level03"), "level03.toml");
        assert_eq!(normalize_filename("level03.toml"), "level03.toml");
        assert_eq!(normalize_filename("map_dir/foo"), "map_dir/foo.toml");
        // An emptied filename must not collapse to a bare `.toml` path.
        assert_eq!(normalize_filename(""), "new_map.toml");
    }

    #[test]
    fn backed_up_filename_cannot_yield_a_bare_toml_path() {
        let mut m = base_model();
        let len = m.file_name.len();
        for _ in 0..len {
            m.backspace_filename();
        }
        // Fully backspaced -> falls back to the default name, not ".toml".
        assert_eq!(m.filename(), "new_map.toml");
    }

    #[test]
    fn save_path_resolves_explicit_vs_default() {
        let m = base_model();
        assert_eq!(m.save_path(Some("maps/mine")), "maps/mine.toml");
        assert_eq!(m.save_path(None), "assets/maps/test.toml");
    }

    #[test]
    fn save_writes_a_file_and_round_trips() {
        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join("exc_editor_save_test.toml");
        let path_str = path.to_str().unwrap().to_string();
        let _ = std::fs::remove_file(&path);

        let mut m = base_model();
        let saved = m.save_to(&path_str).expect("default config is valid");
        assert!(std::path::Path::new(&saved).exists());
        let text = std::fs::read_to_string(&saved).expect("file readable");
        assert!(text.contains("width = 30"));

        // Load back into a fresh model and compare.
        let mut m2 = base_model();
        m2.load_from(&path_str).expect("loads its own save");
        assert_eq!(m2.cfg().width, 30);
        assert_eq!(m2.cfg().start, Pos { x: 15, y: 19 });
        assert_eq!(m2.cfg().seed, None);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_does_not_write_an_invalid_config() {
        let tmp_dir = std::env::temp_dir();
        let path = tmp_dir.join("exc_editor_invalid_test.toml");
        let path_str = path.to_str().unwrap().to_string();
        let _ = std::fs::remove_file(&path);

        let mut m = base_model();
        m.cfg.exit = Pos { x: 5, y: 4 };
        assert!(m.save_to(&path_str).is_err());
        assert!(!path.exists(), "an invalid map must never be written");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_toml_clamps_cursor_and_resets_field_selection() {
        let mut m = base_model();
        m.move_cursor(5, 5);
        m.field_index = 3;
        let text = m.to_toml().unwrap();
        m.move_cursor(9, 9);
        m.load_toml(&text).unwrap();
        assert_eq!(m.cursor(), (15, 19), "cursor resets to the start gap");
        assert_eq!(m.field_index(), 0, "field selection resets");
    }

    #[test]
    fn saved_config_still_generates_a_map() {
        // The editor's output must be consumable by the game's generation.
        use crate::game::generation;
        let mut m = base_model();
        // Add a couple of structures so the round-trip is non-trivial.
        m.set_tool(Tool::Structure);
        for (sx, sy) in [(8, 5), (9, 5)] {
            let c = m.cursor();
            m.move_cursor(sx - c.0, sy - c.1);
            m.apply_tool();
        }
        let text = m.serialize_checked().expect("valid");
        let cfg = MapConfig::from_toml(&text).expect("parse back");
        assert!(
            generation::generate(&cfg, cfg.seed.unwrap_or(0)).is_ok(),
            "saved map must generate"
        );
    }
}
