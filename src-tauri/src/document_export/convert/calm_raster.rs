//! DXP-FR-WGLT, DXP-FR-DMUH: the raster of the SVG drawing of a CALM fence.
//!
//! The frontend makes the drawing (CAD-FR-QHEJ). The SVG comes from the
//! window, so the raster treats it as data: it reads no file, makes no network
//! request, and sets text only in the bundled Liberation faces.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg::{self, fontdb, ImageHrefResolver};

use super::diagrams::{raster_scale, DiagramFailure, MAX_RASTER_PIXELS, MAX_RASTER_SIDE};
use super::pdf_lines;
use super::Picture;

/// DXP-FR-DMUH: the largest SVG the raster reads, in bytes.
pub const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;

/// DXP-FR-DMUH: the largest width or height of an SVG, in CSS pixels.
pub const MAX_SVG_SIDE: f32 = 8000.0;

/// DXP-FR-DMUH: the bundled faces, and no host font.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            for (_, bytes) in pdf_lines::FONT_FILES {
                db.load_font_data(bytes.to_vec());
            }
            db.set_sans_serif_family("Liberation Sans");
            db.set_serif_family("Liberation Sans");
            db.set_monospace_family("Liberation Mono");
            Arc::new(db)
        })
        .clone()
}

/// DXP-FR-DMUH: options that resolve no resource outside the SVG.
fn options() -> usvg::Options<'static> {
    usvg::Options {
        resources_dir: None,
        font_family: "Liberation Sans".to_string(),
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        fontdb: fonts(),
        ..usvg::Options::default()
    }
}

/// DXP-FR-DMUH: the pixel size of a raster of `width` × `height` at `scale`.
/// It rounds to the nearest pixel, and rounds down where the nearest pixel
/// would pass a raster limit.
fn raster_size(width: f32, height: f32, scale: f32) -> (u32, u32) {
    let side = |v: f32, round: fn(f32) -> f32| (round(v * scale) as u32).clamp(1, MAX_RASTER_SIDE);
    let (w, h) = (side(width, f32::round), side(height, f32::round));
    if u64::from(w) * u64::from(h) <= MAX_RASTER_PIXELS {
        return (w, h);
    }
    (side(width, f32::floor), side(height, f32::floor))
}

/// DXP-FR-WGLT: the picture of `svg` at `RASTER_SCALE` times its size, scaled
/// down to keep in the raster limits (`raster_scale`), or `Invalid` for an SVG
/// that does not rasterise (DXP-FR-DMUH). Its natural size is the size of the
/// SVG.
pub fn rasterise_svg(svg: &str) -> Result<Picture, DiagramFailure> {
    if svg.trim().is_empty() || svg.len() > MAX_SVG_BYTES {
        return Err(DiagramFailure::Invalid);
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Option<Picture> {
        let tree = usvg::Tree::from_str(svg, &options()).ok()?;
        let size = tree.size();
        let in_bounds = |side: f32| (1.0..=MAX_SVG_SIDE).contains(&side);
        if !in_bounds(size.width()) || !in_bounds(size.height()) {
            return None;
        }
        let scale = raster_scale(size.width(), size.height());
        let (width, height) = raster_size(size.width(), size.height(), scale);
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let png = pixmap.encode_png().ok()?;
        Some(Picture {
            png,
            width: (size.width().round() as u32).max(1),
            height: (size.height().round() as u32).max(1),
        })
    }));
    match outcome {
        Ok(Some(picture)) if !picture.png.is_empty() => Ok(picture),
        _ => Err(DiagramFailure::Invalid),
    }
}
