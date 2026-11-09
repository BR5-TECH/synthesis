//! DXP-FR-WGLT, DXP-FR-DMUH: the raster of the SVG drawing of a CALM fence.
//!
//! The frontend makes the drawing (CAD-FR-QHEJ). The SVG comes from the
//! window, so the raster treats it as data: it reads no file, makes no network
//! request, and sets text only in the bundled Liberation faces.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};

use resvg::tiny_skia;
use resvg::usvg::{self, fontdb, ImageHrefResolver};

use super::diagrams::{DiagramFailure, RASTER_SCALE};
use super::pdf_lines;

/// DXP-FR-DMUH: the largest SVG the raster reads, in bytes.
pub const MAX_SVG_BYTES: usize = 2 * 1024 * 1024;

/// DXP-FR-DMUH: the largest width or height of an SVG, in CSS pixels.
pub const MAX_SVG_SIDE: f32 = 8000.0;

/// DXP-FR-DMUH: the longest side of a raster, in pixels. A larger raster adds
/// no detail on a page, and it keeps one raster at about 64 MB at most.
pub const MAX_RASTER_SIDE: f32 = 4000.0;

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

/// DXP-FR-WGLT: the PNG bytes of `svg` at `RASTER_SCALE` times its size,
/// scaled down so that its longest side is at most `MAX_RASTER_SIDE`, or
/// `Invalid` for an SVG that does not rasterise (DXP-FR-DMUH).
pub fn rasterise_svg(svg: &str) -> Result<Vec<u8>, DiagramFailure> {
    if svg.trim().is_empty() || svg.len() > MAX_SVG_BYTES {
        return Err(DiagramFailure::Invalid);
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| -> Option<Vec<u8>> {
        let tree = usvg::Tree::from_str(svg, &options()).ok()?;
        let size = tree.size();
        let in_bounds = |side: f32| (1.0..=MAX_SVG_SIDE).contains(&side);
        if !in_bounds(size.width()) || !in_bounds(size.height()) {
            return None;
        }
        let longest = size.width().max(size.height());
        let scale = RASTER_SCALE.min(MAX_RASTER_SIDE / longest);
        let width = ((size.width() * scale).round() as u32).max(1);
        let height = ((size.height() * scale).round() as u32).max(1);
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        pixmap.encode_png().ok()
    }));
    match outcome {
        Ok(Some(png)) if !png.is_empty() => Ok(png),
        _ => Err(DiagramFailure::Invalid),
    }
}
