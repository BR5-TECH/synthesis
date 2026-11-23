//! The pages of a `docx` export: their size, the box a picture fits in, and
//! the landscape section of a wide diagram (DXP-FR-HKFT, DXP-FR-LQWZ).
//!
//! Every page is A4 with the default margins of `docx-rs`. A portrait page
//! is the default, and a landscape page has the same margins.

use docx_rs::{PageOrientationType, PageSize, SectionProperty};

use super::typography as ty;
use super::Picture;

/// The short and the long side of an A4 page, in twentieths of a point.
const A4_SHORT_TWIPS: u32 = 11_906;
const A4_LONG_TWIPS: u32 = 16_838;
/// The margins of the default `docx-rs` page, in twentieths of a point.
const MARGIN_TOP_TWIPS: u32 = 1_985;
const MARGIN_BOTTOM_TWIPS: u32 = 1_701;
const MARGIN_SIDE_TWIPS: u32 = 1_701;
/// EMU per twentieth of a point.
const EMU_PER_TWIP: u64 = 635;
/// EMU per pixel at 96 dpi.
pub const EMU_PER_PX: u64 = 9_525;
/// DXP-FR-LQWZ: a diagram that the portrait page shows at this scale or
/// larger stays on the portrait page.
pub const LANDSCAPE_BELOW_SCALE: f64 = 0.75;

/// The orientation of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// The box that a picture fits in on a page, in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBox {
    pub width_emu: u64,
    pub height_emu: u64,
}

/// The width and the height of a page of `orientation`, in twentieths of a
/// point.
fn page_twips(orientation: Orientation) -> (u32, u32) {
    match orientation {
        Orientation::Portrait => (A4_SHORT_TWIPS, A4_LONG_TWIPS),
        Orientation::Landscape => (A4_LONG_TWIPS, A4_SHORT_TWIPS),
    }
}

/// The text width of a page of `orientation`, in twentieths of a point.
pub fn text_width_twips(orientation: Orientation) -> u32 {
    page_twips(orientation).0 - 2 * MARGIN_SIDE_TWIPS
}

/// DXP-FR-HKFT: the box of a picture on a page of `orientation`. Its width is
/// the text width. Its height is the usable height: the page height less the
/// top and bottom margins, the space after the paragraph of the picture, and
/// one body line.
pub fn page_box(orientation: Orientation) -> PageBox {
    let height = page_twips(orientation).1 - MARGIN_TOP_TWIPS - MARGIN_BOTTOM_TWIPS;
    let reserved =
        ty::twips(ty::PARAGRAPH_AFTER_PT) + ty::twips(ty::BODY_PT * ty::BODY_LINE_HEIGHT);
    PageBox {
        width_emu: u64::from(text_width_twips(orientation)) * EMU_PER_TWIP,
        height_emu: u64::from(height.saturating_sub(reserved)) * EMU_PER_TWIP,
    }
}

impl PageBox {
    /// The box of a paragraph that is indented by `indent_twips` from the
    /// left margin.
    pub fn indented(self, indent_twips: u32) -> PageBox {
        let indent = u64::from(indent_twips) * EMU_PER_TWIP;
        PageBox { width_emu: self.width_emu.saturating_sub(indent).max(1), ..self }
    }
}

/// The natural size of `picture`, in EMU.
fn natural_emu(picture: &Picture) -> (u64, u64) {
    (u64::from(picture.width.max(1)) * EMU_PER_PX, u64::from(picture.height.max(1)) * EMU_PER_PX)
}

/// DXP-FR-HKFT: the scale that fits `picture` in `frame`, at most 1, so a
/// picture is never larger than its natural size.
fn fit_scale(picture: &Picture, frame: PageBox) -> f64 {
    let (w, h) = natural_emu(picture);
    let by_width = frame.width_emu as f64 / w as f64;
    let by_height = frame.height_emu as f64 / h as f64;
    by_width.min(by_height).min(1.0)
}

/// DXP-FR-HKFT: the size of `picture` in `frame`, in EMU. It keeps the aspect
/// ratio, and it is the natural size where the picture fits.
pub fn fit(picture: &Picture, frame: PageBox) -> (u32, u32) {
    let (w, h) = natural_emu(picture);
    if w <= frame.width_emu && h <= frame.height_emu {
        return (clamp_u32(w), clamp_u32(h));
    }
    let scale = fit_scale(picture, frame);
    let fitted = |side: u64| ((side as f64 * scale).floor() as u64).max(1);
    (clamp_u32(fitted(w).min(frame.width_emu)), clamp_u32(fitted(h).min(frame.height_emu)))
}

fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// DXP-FR-LQWZ: the page of a diagram in a paragraph indented by
/// `indent_twips`. A diagram goes on a landscape page when the portrait page
/// shows it at less than `LANDSCAPE_BELOW_SCALE` of its natural size and the
/// landscape page shows it at a larger scale.
pub fn diagram_orientation(picture: &Picture, indent_twips: u32) -> Orientation {
    let portrait = fit_scale(picture, page_box(Orientation::Portrait).indented(indent_twips));
    if portrait >= LANDSCAPE_BELOW_SCALE {
        return Orientation::Portrait;
    }
    let landscape = fit_scale(picture, page_box(Orientation::Landscape).indented(indent_twips));
    if landscape > portrait {
        Orientation::Landscape
    } else {
        Orientation::Portrait
    }
}

/// DXP-FR-LQWZ: the section properties of a page of `orientation`, with the
/// default margins of `docx-rs`.
pub fn section(orientation: Orientation) -> SectionProperty {
    let (w, h) = page_twips(orientation);
    let orient = match orientation {
        Orientation::Portrait => PageOrientationType::Portrait,
        Orientation::Landscape => PageOrientationType::Landscape,
    };
    SectionProperty::new().page_size(PageSize::new().size(w, h).orient(orient))
}
