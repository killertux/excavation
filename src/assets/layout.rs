//! Pure asset-sheet detection and slicing (no GPU, no macroquad).
//!
//! This module decodes an RGBA image, splits it into a uniform `rows × cols`
//! grid (the committed atlases are tightly-packed modular grids with no
//! transparent gutters), crops each cell, and rescales the result to a
//! [`TILE_SIZE`] RGBA buffer. It is fully unit-testable without a window.
//!
//! ## Layout model
//!
//! The committed sheets are production atlases packed as a uniform `rows × cols`
//! grid, so detection splits each axis into equal cells and crops each cell.
//! Sheets that are not a clean grid must supply `SheetSpec::explicit_rects`.

use image::RgbaImage;
use image::imageops::FilterType;

/// Target tile size, in pixels (each axis). Matches the atlas cell size so the
/// combined atlas is sliced at native resolution (no resampling blur).
pub const TILE_SIZE: u32 = 32;

/// How to map a cropped source region onto the [`TILE_SIZE`] tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    /// Non-uniform resize to exactly `TILE_SIZE` (fills the tile; may distort).
    Stretch,
    /// Aspect-preserving resize, centered on the tile with transparent padding.
    Fit,
}

/// An integer pixel rectangle (crop region), in source-image coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
}

/// A single sliced 16×16 RGBA frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Zero-based linear index of the frame within its sheet (`row * cols + col`).
    pub index: usize,
    /// Source-grid row of this frame.
    pub row: usize,
    /// Source-grid column of this frame.
    pub col: usize,
    /// 16×16 RGBA pixels, row-major, 4 bytes per pixel (`16*16*4 == 1024`).
    pub rgba: Vec<u8>,
}

/// Description of how to slice one sheet.
#[derive(Debug, Clone)]
pub struct SheetSpec {
    /// Number of grid rows.
    pub rows: usize,
    /// Number of grid columns.
    pub cols: usize,
    /// Resize strategy applied to each cropped cell.
    pub scale_mode: ScaleMode,
    /// Optional explicit crop rects, in source-image coordinates, in row-major
    /// order. When present these replace auto grid-splitting.
    pub explicit_rects: Option<Vec<Rect>>,
}

impl SheetSpec {
    /// A uniform-grid spec (no explicit rects). Used by the grid-slicing tests
    /// and the generic slice path; the atlas loader supplies explicit rects.
    #[allow(dead_code)]
    pub fn new(rows: usize, cols: usize, scale_mode: ScaleMode) -> Self {
        SheetSpec {
            rows,
            cols,
            scale_mode,
            explicit_rects: None,
        }
    }
}

/// Errors produced while slicing a sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// The image contained no opaque content.
    NoContent,
    /// An explicit rect fell outside the image bounds.
    RectOutOfBounds {
        index: usize,
        rect: Rect,
        width: u32,
        height: u32,
    },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::NoContent => write!(f, "sheet has no opaque content"),
            LayoutError::RectOutOfBounds {
                index,
                rect,
                width,
                height,
            } => {
                write!(
                    f,
                    "explicit rect {index} ({},{},{},{}) out of bounds for {width}x{height}",
                    rect.x, rect.y, rect.w, rect.h
                )
            }
        }
    }
}

impl std::error::Error for LayoutError {}

/// Slice `img` into 16×16 frames according to `spec`.
///
/// Returns one `Frame` per grid cell, in row-major order (`row * cols + col`).
pub fn detect_and_resize(img: &RgbaImage, spec: &SheetSpec) -> Result<Vec<Frame>, LayoutError> {
    let (width, height) = img.dimensions();

    let rects: Vec<Rect> = match &spec.explicit_rects {
        Some(rects) => {
            for (i, r) in rects.iter().enumerate() {
                if r.x + r.w > width || r.y + r.h > height {
                    return Err(LayoutError::RectOutOfBounds {
                        index: i,
                        rect: *r,
                        width,
                        height,
                    });
                }
            }
            rects.clone()
        }
        None => grid_rects(width, height, spec.rows, spec.cols),
    };

    if rects.is_empty() {
        return Err(LayoutError::NoContent);
    }
    let expected = spec.rows * spec.cols;
    if spec.explicit_rects.is_none() && rects.len() != expected {
        return Err(LayoutError::NoContent);
    }

    Ok(rects
        .iter()
        .enumerate()
        .map(|(index, r)| Frame {
            index,
            row: index / spec.cols,
            col: index % spec.cols,
            rgba: crop_and_resize(img, r, spec.scale_mode),
        })
        .collect())
}

/// Split `w × h` into a uniform `rows × cols` grid of crop rects (row-major).
fn grid_rects(width: u32, height: u32, rows: usize, cols: usize) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        let y0 = height * r as u32 / rows as u32;
        let y1 = height * (r + 1) as u32 / rows as u32;
        for c in 0..cols {
            let x0 = width * c as u32 / cols as u32;
            let x1 = width * (c + 1) as u32 / cols as u32;
            rects.push(Rect::new(x0, y0, x1 - x0, y1 - y0));
        }
    }
    rects
}

/// Crop `rect` from `img` and resize the result to a [`TILE_SIZE`] RGBA buffer.
fn crop_and_resize(img: &RgbaImage, rect: &Rect, scale_mode: ScaleMode) -> Vec<u8> {
    let crop = image::imageops::crop_imm(img, rect.x, rect.y, rect.w, rect.h).to_image();
    let out = match scale_mode {
        ScaleMode::Stretch => {
            image::imageops::resize(&crop, TILE_SIZE, TILE_SIZE, FilterType::Nearest)
        }
        ScaleMode::Fit => fit_resize(&crop, TILE_SIZE, TILE_SIZE),
    };
    out.into_raw()
}

/// Resize preserving aspect ratio, centered on a `target`×`target` canvas.
fn fit_resize(img: &RgbaImage, target_w: u32, target_h: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = (target_w as f32 / w as f32).min(target_h as f32 / h as f32);
    let nw = ((w as f32 * scale).round() as u32).max(1);
    let nh = ((h as f32 * scale).round() as u32).max(1);

    let resized = image::imageops::resize(img, nw, nh, FilterType::Nearest);

    let mut canvas = RgbaImage::new(target_w, target_h);
    let ox = ((target_w - nw) / 2) as i64;
    let oy = ((target_h - nh) / 2) as i64;
    image::imageops::overlay(&mut canvas, &resized, ox, oy);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(path: &str) -> RgbaImage {
        image::open(path)
            .expect("test asset should exist")
            .into_rgba8()
    }

    #[test]
    fn atlas_explicit_rects_slice_upwards() {
        // The committed atlas is used with explicit rects (not a uniform grid),
        // so slicing via explicit_rects is the operative path. Crop the dirt base
        // cell (32 px at origin (0, 330)) and confirm it downscales to 16×16.
        let img = load("assets/images/My project atlas.png");
        let spec = SheetSpec {
            rows: 1,
            cols: 1,
            scale_mode: ScaleMode::Stretch,
            explicit_rects: Some(vec![Rect::new(0, 330, 32, 32)]),
        };
        let frames = detect_and_resize(&img, &spec).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].index, 0);
        assert_eq!(frames[0].rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);
    }

    #[test]
    fn grid_rows_are_row_major() {
        let img = RgbaImage::new(30, 20);
        let spec = SheetSpec::new(4, 5, ScaleMode::Stretch);
        let frames = detect_and_resize(&img, &spec).unwrap();
        // 4 rows x 5 cols; index 6 -> row 1, col 1.
        assert_eq!(frames.len(), 20);
        assert_eq!(frames[6].row, 1);
        assert_eq!(frames[6].col, 1);
        assert_eq!(frames[0].index, 0);
        assert_eq!(frames[19].index, 19);
    }

    #[test]
    fn stretch_and_fit_resize_to_16x16() {
        // A small non-square synthetic image (e.g. 8x4).
        let mut img = RgbaImage::new(8, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([x as u8, y as u8, 255, 255]);
        }
        let spec_stretch = SheetSpec::new(2, 2, ScaleMode::Stretch);
        let f = detect_and_resize(&img, &spec_stretch).unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f[0].rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);

        // Fit must also produce a 16x16 canvas with transparent padding.
        let spec_fit = SheetSpec::new(2, 2, ScaleMode::Fit);
        let f2 = detect_and_resize(&img, &spec_fit).unwrap();
        assert_eq!(f2[0].rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);

        // Verify the two modes produce different bytes (Stretch fills the tile
        // fully; Fit leaves transparent padding on the wider axis).
        assert_ne!(f2[0].rgba, f[0].rgba);
    }

    #[test]
    fn explicit_rects_override_detection() {
        let img = load("assets/images/My project atlas.png");
        let spec = SheetSpec {
            rows: 1,
            cols: 2,
            scale_mode: ScaleMode::Stretch,
            explicit_rects: Some(vec![
                Rect::new(0, 330, 32, 32),  // dirt base
                Rect::new(33, 330, 32, 32), // (unused cell to its right)
            ]),
        };
        let frames = detect_and_resize(&img, &spec).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].index, 0);
        assert_eq!(frames[1].index, 1);
    }

    #[test]
    fn explicit_rect_out_of_bounds_errors() {
        let img = load("assets/images/My project atlas.png");
        let (w, h) = img.dimensions();
        let spec = SheetSpec {
            rows: 1,
            cols: 1,
            scale_mode: ScaleMode::Stretch,
            explicit_rects: Some(vec![Rect::new(w, h, 10, 10)]),
        };
        assert!(matches!(
            detect_and_resize(&img, &spec),
            Err(LayoutError::RectOutOfBounds { .. })
        ));
    }
}
