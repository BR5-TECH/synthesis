//! Diagram fences rendered to PNG images (DXP-FR-RKJC, DXP-FR-WGLT,
//! DXP-FR-IOFM).
//!
//! A Mermaid fence renders with `merman`, in the backend and with no webview.
//! A CALM fence is rasterised from the drawing the frontend gives
//! (`calm_raster`). A diagram that does not render keeps its source, and the
//! export reports it as a warning.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use merman::{OperationControl, PngRequest, RenderOutput, RenderRequest, Renderer, SvgRequest};

use super::model::{Block, DiagramKind, Document};
use super::Picture;

/// The largest diagram source the export renders, in bytes.
const MAX_SOURCE_BYTES: usize = 256 * 1024;

/// DXP-FR-QCEB: the time one Mermaid diagram may take to render.
pub const RENDER_DEADLINE: Duration = Duration::from_secs(20);

/// DXP-FR-RKJC, DXP-FR-DMUH: the raster scale. A PNG at three times its
/// layout size stays sharp when the reader zooms in.
pub const RASTER_SCALE: f32 = 3.0;

/// DXP-FR-RKJC, DXP-FR-DMUH: the longest side of a raster, in pixels. It is
/// about 300 dpi across the text width of a landscape page.
pub const MAX_RASTER_SIDE: u32 = 3000;

/// DXP-FR-RKJC, DXP-FR-DMUH: the largest area of a raster, in pixels.
pub const MAX_RASTER_PIXELS: u64 = 6_000_000;

/// Why a diagram stayed as code (DXP-FR-IOFM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramFailure {
    /// It does not render, it renders to nothing, or its renderer panics.
    Invalid,
    /// DXP-FR-QCEB: its render did not end before the deadline.
    Deadline,
}

impl DiagramFailure {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagramFailure::Invalid => "invalid",
            DiagramFailure::Deadline => "deadline",
        }
    }
}

/// One diagram that stayed as code, counted from 1 in document order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FailedDiagram {
    pub index: usize,
    pub reason: DiagramFailure,
}

/// The renderer of one diagram of a kind: its picture, or why it failed.
pub type DiagramRenderer<'a> = &'a dyn Fn(DiagramKind, &str) -> Result<Picture, DiagramFailure>;

/// DXP-FR-RKJC: the picture of one diagram, or the reason it stays as code
/// (DXP-FR-IOFM). The render stops at its deadline (DXP-FR-QCEB).
pub fn render_png(source: &str) -> Result<Picture, DiagramFailure> {
    render_png_within(source, RENDER_DEADLINE)
}

/// DXP-FR-RKJC, DXP-FR-DMUH: the scale of a raster of `width` × `height`
/// CSS pixels: `RASTER_SCALE`, or less so that the raster keeps in
/// `MAX_RASTER_SIDE` and `MAX_RASTER_PIXELS`.
pub fn raster_scale(width: f32, height: f32) -> f32 {
    let longest = width.max(height).max(1.0);
    let area = (width * height).max(1.0);
    RASTER_SCALE.min(MAX_RASTER_SIDE as f32 / longest).min((MAX_RASTER_PIXELS as f32 / area).sqrt())
}

/// `render_png` with the deadline `limit`.
pub fn render_png_within(source: &str, limit: Duration) -> Result<Picture, DiagramFailure> {
    if source.trim().is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err(DiagramFailure::Invalid);
    }
    let control = OperationControl::new().with_deadline(limit);
    let render_control = control.clone();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let limit = merman_export::RasterSizeLimit::new(
            Some(MAX_RASTER_SIDE),
            Some(MAX_RASTER_SIDE),
            Some(MAX_RASTER_PIXELS),
        );
        let options = merman_export::RasterOptions {
            scale: RASTER_SCALE,
            size_limit: limit,
            ..Default::default()
        };
        let request = PngRequest { svg: SvgRequest::default(), options };
        Renderer::new().render(RenderRequest::png(source, render_control, request))
    }));
    match outcome {
        Ok(Ok(RenderOutput::Png(Some(raster)))) if !raster.bytes().is_empty() => {
            // DXP-FR-RKJC: the natural size is the layout size, also for a
            // raster that the limits scale down.
            let plan = raster.plan();
            let natural = |px: f64| (px / plan.requested_scale).round().max(1.0) as u32;
            Ok(Picture {
                png: raster.into_bytes(),
                width: natural(plan.requested_width_px),
                height: natural(plan.requested_height_px),
            })
        }
        // DXP-FR-QCEB: the render stops at a checkpoint once the deadline is
        // past. The control then reports the deadline, whatever error the
        // render gave.
        _ if control.checkpoint().is_err() => Err(DiagramFailure::Deadline),
        _ => Err(DiagramFailure::Invalid),
    }
}

/// DXP-FR-RKJC / DXP-FR-IOFM: render every diagram of `doc` in document order
/// with `render`. `progress(done, total)` follows each diagram. Returns the
/// diagrams that stayed as code.
pub fn render_all(
    doc: &mut Document,
    render: DiagramRenderer<'_>,
    progress: &mut dyn FnMut(usize, usize),
) -> Vec<FailedDiagram> {
    let total = doc.diagram_count();
    let mut failed = Vec::new();
    let mut done = 0;
    for top in &mut doc.blocks {
        visit(&mut top.block, render, &mut |outcome| {
            done += 1;
            if let Some(failure) = outcome {
                failed.push(failure);
            }
            progress(done, total);
        });
    }
    failed
}

fn visit(
    block: &mut Block,
    render: DiagramRenderer<'_>,
    report: &mut dyn FnMut(Option<FailedDiagram>),
) {
    match block {
        Block::Diagram { index, kind, source, image } => match render(*kind, source) {
            Ok(picture) => {
                *image = Some(picture);
                report(None);
            }
            Err(reason) => {
                *image = None;
                report(Some(FailedDiagram { index: *index, reason }));
            }
        },
        Block::Quote(inner) => inner.iter_mut().for_each(|b| visit(b, render, report)),
        Block::List { items, .. } => items
            .iter_mut()
            .flat_map(|i| i.blocks.iter_mut())
            .for_each(|b| visit(b, render, report)),
        _ => {}
    }
}

/// DXP-FR-TQCQ: the sources of the CALM fences of `doc`, each one time, in
/// the document order of its first fence.
pub fn calm_sources(doc: &Document) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    for top in &doc.blocks {
        collect_calm(&top.block, &mut sources);
    }
    sources
}

fn collect_calm(block: &Block, sources: &mut Vec<String>) {
    match block {
        Block::Diagram { kind: DiagramKind::Calm, source, .. }
            if !sources.iter().any(|s| s == source) =>
        {
            sources.push(source.clone());
        }
        Block::Quote(inner) => inner.iter().for_each(|b| collect_calm(b, sources)),
        Block::List { items, .. } => {
            items.iter().flat_map(|i| i.blocks.iter()).for_each(|b| collect_calm(b, sources))
        }
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
