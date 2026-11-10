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

/// The largest diagram source the export renders, in bytes.
const MAX_SOURCE_BYTES: usize = 256 * 1024;

/// DXP-FR-QCEB: the time one Mermaid diagram may take to render.
pub const RENDER_DEADLINE: Duration = Duration::from_secs(20);

/// The raster scale: a PNG at twice its layout size stays sharp on a page.
pub const RASTER_SCALE: f32 = 2.0;

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

/// The renderer of one diagram of a kind: its PNG bytes, or why it failed.
pub type DiagramRenderer<'a> = &'a dyn Fn(DiagramKind, &str) -> Result<Vec<u8>, DiagramFailure>;

/// DXP-FR-RKJC: the PNG bytes of one diagram, or the reason it stays as code
/// (DXP-FR-IOFM). The render stops at its deadline (DXP-FR-QCEB).
pub fn render_png(source: &str) -> Result<Vec<u8>, DiagramFailure> {
    render_png_within(source, RENDER_DEADLINE)
}

/// `render_png` with the deadline `limit`.
pub fn render_png_within(source: &str, limit: Duration) -> Result<Vec<u8>, DiagramFailure> {
    if source.trim().is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err(DiagramFailure::Invalid);
    }
    let control = OperationControl::new().with_deadline(limit);
    let render_control = control.clone();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let options = merman_export::RasterOptions { scale: RASTER_SCALE, ..Default::default() };
        let request = PngRequest { svg: SvgRequest::default(), options };
        Renderer::new().render(RenderRequest::png(source, render_control, request))
    }));
    match outcome {
        Ok(Ok(RenderOutput::Png(Some(raster)))) if !raster.bytes().is_empty() => {
            Ok(raster.bytes().to_vec())
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
        Block::Diagram { index, kind, source, png } => match render(*kind, source) {
            Ok(bytes) => {
                *png = Some(bytes);
                report(None);
            }
            Err(reason) => {
                *png = None;
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
