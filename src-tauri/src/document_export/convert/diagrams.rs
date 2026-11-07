//! Diagram fences rendered to PNG images with `merman` (DXP-FR-RKJC,
//! DXP-FR-IOFM).
//!
//! The render runs in the backend and with no webview. A diagram that does not
//! render keeps its source, and the export reports it as a warning.

use std::panic::{catch_unwind, AssertUnwindSafe};

use merman::{OperationControl, PngRequest, RenderOutput, RenderRequest, Renderer, SvgRequest};

use super::model::{Block, Document};

/// The largest diagram source the export renders, in bytes.
const MAX_SOURCE_BYTES: usize = 256 * 1024;

/// The raster scale: a PNG at twice its layout size stays sharp on a page.
pub const RASTER_SCALE: f32 = 2.0;

/// DXP-FR-RKJC: the PNG bytes of one diagram, or `None` when it does not
/// render, renders to nothing, or its renderer panics (DXP-FR-IOFM).
pub fn render_png(source: &str) -> Option<Vec<u8>> {
    if source.trim().is_empty() || source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let options = merman_export::RasterOptions { scale: RASTER_SCALE, ..Default::default() };
        let request = PngRequest { svg: SvgRequest::default(), options };
        Renderer::new().render(RenderRequest::png(source, OperationControl::new(), request))
    }));
    match outcome {
        Ok(Ok(RenderOutput::Png(Some(raster)))) if !raster.bytes().is_empty() => {
            Some(raster.bytes().to_vec())
        }
        _ => None,
    }
}

/// DXP-FR-RKJC / DXP-FR-IOFM: render every diagram of `doc` in document order
/// with `render`. `progress(done, total)` follows each diagram. Returns the
/// indexes of the diagrams that stayed as code.
pub fn render_all(
    doc: &mut Document,
    render: &dyn Fn(&str) -> Option<Vec<u8>>,
    progress: &mut dyn FnMut(usize, usize),
) -> Vec<usize> {
    let total = doc.diagram_count();
    let mut failed = Vec::new();
    let mut done = 0;
    for top in &mut doc.blocks {
        visit(&mut top.block, render, &mut |ok, index| {
            done += 1;
            if !ok {
                failed.push(index);
            }
            progress(done, total);
        });
    }
    failed
}

fn visit(
    block: &mut Block,
    render: &dyn Fn(&str) -> Option<Vec<u8>>,
    report: &mut dyn FnMut(bool, usize),
) {
    match block {
        Block::Diagram { index, source, png } => {
            *png = render(source);
            report(png.is_some(), *index);
        }
        Block::Quote(inner) => inner.iter_mut().for_each(|b| visit(b, render, report)),
        Block::List { items, .. } => items
            .iter_mut()
            .flat_map(|i| i.blocks.iter_mut())
            .for_each(|b| visit(b, render, report)),
        _ => {}
    }
}

/// The pixel size of a PNG, read from its header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}
