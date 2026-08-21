//! Pure asset-sheet detection and slicing (no GPU, no macroquad).
//!
//! This module decodes an RGBA image, detects the frame/tile regions in a
//! single-row atlas, trims each to its tight bounding box, and rescales the
//! result to a 16×16 RGBA buffer. It is fully unit-testable without a window.
//!
//! ## Layout model
//!
//! The committed sheets are **one row of frames, left-to-right, separated by
//! transparent gutters**, with transparent vertical margins. Detection therefore
//! works by scanning columns for opaque content, splitting on fully-transparent
//! columns, then computing the tight bounding box of each run.
//!
//! Sheets that do not follow this model (e.g. the sparse particles sheet) must
//! supply `SheetSpec::explicit_rects`.

use image::imageops::FilterType;
use image::RgbaImage;

/// Target tile size, in pixels (each axis).
pub const TILE_SIZE: u32 = 16;

/// A pixel is considered opaque for detection/trimming when its alpha is >=
/// this. Robust against faint anti-aliasing while keeping all visible content.
const OPAQUE_THRESHOLD: u8 = 32;

/// How to map a cropped source region onto the 16×16 tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleMode {
    /// Non-uniform resize to exactly 16×16 (fills the tile; may distort aspect).
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
    /// Zero-based index of the frame within its sheet (left-to-right).
    pub index: usize,
    /// 16×16 RGBA pixels, row-major, 4 bytes per pixel (`16*16*4 == 1024`).
    pub rgba: Vec<u8>,
}

/// Description of how to slice one sheet.
#[derive(Debug, Clone)]
pub struct SheetSpec {
    /// The number of frames/tiles this sheet must contain. Detection asserts the
    /// found count matches, so a mis-detected sheet fails loudly.
    pub expected_frames: usize,
    /// Resize strategy applied to each detected/cropped region.
    pub scale_mode: ScaleMode,
    /// Optional explicit crop rects, in source-image coordinates. When present
    /// these replace auto-detection (used for sheets that don't follow the
    /// single-row-gutter layout).
    pub explicit_rects: Option<Vec<Rect>>,
}

impl SheetSpec {
    pub fn new(expected_frames: usize, scale_mode: ScaleMode) -> Self {
        SheetSpec { expected_frames, scale_mode, explicit_rects: None }
    }
}

/// Errors produced while slicing a sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// The image contained no opaque content.
    NoContent,
    /// Detected frame count did not match `SheetSpec::expected_frames`.
    WrongFrameCount { expected: usize, found: usize },
    /// An explicit rect fell outside the image bounds.
    RectOutOfBounds { index: usize, rect: Rect, width: u32, height: u32 },
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::NoContent => write!(f, "sheet has no opaque content"),
            LayoutError::WrongFrameCount { expected, found } => {
                write!(f, "expected {expected} frames, found {found}")
            }
            LayoutError::RectOutOfBounds { index, rect, width, height } => {
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
/// Returns one `Frame` per detected/cropped region, in left-to-right order.
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
        None => detect_bboxes(img),
    };

    if rects.is_empty() {
        return Err(LayoutError::NoContent);
    }
    if rects.len() != spec.expected_frames {
        return Err(LayoutError::WrongFrameCount {
            expected: spec.expected_frames,
            found: rects.len(),
        });
    }

    Ok(rects
        .iter()
        .enumerate()
        .map(|(index, r)| Frame {
            index,
            rgba: crop_and_resize(img, r, spec.scale_mode),
        })
        .collect())
}

/// Detect the tight bounding box of each content region in a single-row atlas.
fn detect_bboxes(img: &RgbaImage) -> Vec<Rect> {
    let (width, height) = img.dimensions();

    // Per-column "has opaque content" flag.
    let mut col_content = vec![false; width as usize];
    for x in 0..width {
        for y in 0..height {
            if img.get_pixel(x, y)[3] >= OPAQUE_THRESHOLD {
                col_content[x as usize] = true;
                break;
            }
        }
    }

    let mut rects = Vec::new();
    let mut x = 0u32;
    while x < width {
        if col_content[x as usize] {
            let start = x;
            while x < width && col_content[x as usize] {
                x += 1;
            }
            let end = x - 1;

            // Tight bounding box within this column run.
            let mut minx = u32::MAX;
            let mut miny = u32::MAX;
            let mut maxx = 0u32;
            let mut maxy = 0u32;
            for cx in start..=end {
                for cy in 0..height {
                    if img.get_pixel(cx, cy)[3] >= OPAQUE_THRESHOLD {
                        if cx < minx {
                            minx = cx;
                        }
                        if cx > maxx {
                            maxx = cx;
                        }
                        if cy < miny {
                            miny = cy;
                        }
                        if cy > maxy {
                            maxy = cy;
                        }
                    }
                }
            }
            rects.push(Rect::new(minx, miny, maxx - minx + 1, maxy - miny + 1));
        } else {
            x += 1;
        }
    }
    rects
}

/// Crop `rect` from `img` and resize the result to a 16×16 RGBA buffer.
fn crop_and_resize(img: &RgbaImage, rect: &Rect, scale_mode: ScaleMode) -> Vec<u8> {
    let crop = image::imageops::crop_imm(img, rect.x, rect.y, rect.w, rect.h).to_image();
    let out = match scale_mode {
        ScaleMode::Stretch => {
            image::imageops::resize(&crop, TILE_SIZE, TILE_SIZE, FilterType::Triangle)
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

    let resized = image::imageops::resize(img, nw, nh, FilterType::Triangle);

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
        image::open(path).expect("test asset should exist").into_rgba8()
    }

    #[test]
    fn terrain_sheet_detects_seven_tiles_16x16() {
        let img = load("assets/images/tiles/terrain_atlas.png");
        let spec = SheetSpec::new(7, ScaleMode::Stretch);
        let frames = detect_and_resize(&img, &spec).unwrap();
        assert_eq!(frames.len(), 7);
        for f in &frames {
            assert_eq!(f.rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);
        }
    }

    #[test]
    fn player_sheet_detects_four_frames_16x16() {
        let img = load("assets/images/characters/player_sheet.png");
        let spec = SheetSpec::new(4, ScaleMode::Fit);
        let frames = detect_and_resize(&img, &spec).unwrap();
        assert_eq!(frames.len(), 4);
        for f in &frames {
            assert_eq!(f.rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);
        }
    }

    #[test]
    fn stretch_and_fit_resize_to_16x16() {
        // A small non-square synthetic image (e.g. 8x4).
        let mut img = RgbaImage::new(8, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgba([x as u8, y as u8, 255, 255]);
        }
        let spec_stretch = SheetSpec::new(1, ScaleMode::Stretch);
        let f = detect_and_resize(&img, &spec_stretch).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);

        // Fit must also produce a 16x16 canvas with transparent padding.
        let spec_fit = SheetSpec::new(1, ScaleMode::Fit);
        let f = detect_and_resize(&img, &spec_fit).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].rgba.len(), (TILE_SIZE * TILE_SIZE * 4) as usize);

        // Verify the two modes produce different bytes (Stretch fills the tile
        // fully; Fit leaves transparent padding on the wider axis).
        assert_ne!(f[0].rgba, detect_and_resize(&img, &spec_stretch).unwrap()[0].rgba);
    }

    #[test]
    fn wrong_expected_frames_errors() {
        let img = load("assets/images/tiles/terrain_atlas.png");
        let spec = SheetSpec::new(6, ScaleMode::Stretch); // wrong: actual is 7
        assert_eq!(
            detect_and_resize(&img, &spec),
            Err(LayoutError::WrongFrameCount { expected: 6, found: 7 })
        );
    }

    #[test]
    fn empty_image_errors() {
        let img = RgbaImage::new(16, 16); // fully transparent
        let spec = SheetSpec::new(1, ScaleMode::Stretch);
        assert_eq!(detect_and_resize(&img, &spec), Err(LayoutError::NoContent));
    }

    #[test]
    fn explicit_rects_override_detection() {
        let img = load("assets/images/tiles/terrain_atlas.png");
        let spec = SheetSpec {
            expected_frames: 2,
            scale_mode: ScaleMode::Stretch,
            explicit_rects: Some(vec![
                Rect::new(53, 220, 241, 241),
                Rect::new(350, 220, 241, 242),
            ]),
        };
        let frames = detect_and_resize(&img, &spec).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].index, 0);
        assert_eq!(frames[1].index, 1);
    }

    #[test]
    fn explicit_rect_out_of_bounds_errors() {
        let img = load("assets/images/tiles/terrain_atlas.png");
        let (w, h) = img.dimensions();
        let spec = SheetSpec {
            expected_frames: 1,
            scale_mode: ScaleMode::Stretch,
            explicit_rects: Some(vec![Rect::new(w, h, 10, 10)]),
        };
        assert!(matches!(
            detect_and_resize(&img, &spec),
            Err(LayoutError::RectOutOfBounds { .. })
        ));
    }
}
