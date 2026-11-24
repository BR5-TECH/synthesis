//! The pages of a `docx` export: their size and margins, the box a picture
//! fits in, and the landscape section of a wide diagram (DXP-FR-PGMR,
//! DXP-FR-HKFT, DXP-FR-LQWZ).
//!
//! Every page is A4 with the margins of the page setup. A portrait page is
//! the default, and a landscape page has the same margins.

use docx_rs::{PageMargin, PageOrientationType, PageSize, SectionProperty};

use super::typography as ty;
use super::Picture;

/// The short and the long side of an A4 page, in twentieths of a point.
const A4_SHORT_TWIPS: u32 = 11_906;
const A4_LONG_TWIPS: u32 = 16_838;
/// The header and footer distances of the default `docx-rs` page.
const HEADER_TWIPS: i32 = 851;
const FOOTER_TWIPS: i32 = 992;
/// EMU per twentieth of a point.
const EMU_PER_TWIP: u64 = 635;
/// EMU per pixel at 96 dpi.
pub const EMU_PER_PX: u64 = 9_525;

/// GSS-FR-TLHZ: the default landscape threshold, in percent.
pub const DEFAULT_LANDSCAPE_BELOW_PERCENT: u32 = 75;

/// The orientation of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// DXP-FR-PGMR: the margins of a page, in twentieths of a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Margins {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

/// Twentieths of a point in `mm` millimetres, rounded.
pub fn mm_to_twips(mm: u32) -> u32 {
    ((f64::from(mm) * 1440.0 / 25.4).round()) as u32
}

impl Margins {
    /// The margins of the top, bottom, left, and right sides in millimetres.
    pub fn from_mm(top: u32, bottom: u32, left: u32, right: u32) -> Self {
        Margins {
            top: mm_to_twips(top),
            bottom: mm_to_twips(bottom),
            left: mm_to_twips(left),
            right: mm_to_twips(right),
        }
    }
}

impl Default for Margins {
    /// GSS-FR-MGNV: 35 mm at the top and 30 mm on the other sides.
    fn default() -> Self {
        Margins::from_mm(35, 30, 30, 30)
    }
}

/// The page setup of one `docx` export, read from the app preferences when
/// the export starts (DXP-FR-PGMR, DXP-FR-LQWZ).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSetup {
    /// DXP-FR-LQWZ: the **Landscape pages for wide diagrams** switch.
    pub landscape: bool,
    /// DXP-FR-LQWZ: a diagram that the portrait page shows at this scale or
    /// larger stays on the portrait page.
    pub landscape_below: f64,
    /// DXP-FR-PGMR: the margins of every page.
    pub margins: Margins,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup {
            landscape: false,
            landscape_below: f64::from(DEFAULT_LANDSCAPE_BELOW_PERCENT) / 100.0,
            margins: Margins::default(),
        }
    }
}

/// The box that a picture fits in on a page, in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageBox {
    pub width_emu: u64,
    pub height_emu: u64,
}

impl PageBox {
    /// The box of a paragraph that is indented by `indent_twips` from the
    /// left margin.
    pub fn indented(self, indent_twips: u32) -> PageBox {
        let indent = u64::from(indent_twips) * EMU_PER_TWIP;
        PageBox { width_emu: self.width_emu.saturating_sub(indent).max(1), ..self }
    }
}

/// The width and the height of a page of `orientation`, in twentieths of a
/// point.
fn page_twips(orientation: Orientation) -> (u32, u32) {
    match orientation {
        Orientation::Portrait => (A4_SHORT_TWIPS, A4_LONG_TWIPS),
        Orientation::Landscape => (A4_LONG_TWIPS, A4_SHORT_TWIPS),
    }
}

impl PageSetup {
    /// DXP-FR-PGMR: the text width of a page of `orientation`, in twentieths
    /// of a point: the page width less the left and right margins.
    pub fn text_width_twips(&self, orientation: Orientation) -> u32 {
        let sides = self.margins.left + self.margins.right;
        page_twips(orientation).0.saturating_sub(sides).max(1)
    }

    /// DXP-FR-HKFT: the box of a picture on a page of `orientation`. Its width
    /// is the text width. Its height is the usable height: the page height
    /// less the top and bottom margins, the space after the paragraph of the
    /// picture, and one body line.
    pub fn page_box(&self, orientation: Orientation) -> PageBox {
        let margins = self.margins.top + self.margins.bottom;
        let reserved =
            ty::twips(ty::PARAGRAPH_AFTER_PT) + ty::twips(ty::BODY_PT * ty::BODY_LINE_HEIGHT);
        let height = page_twips(orientation).1.saturating_sub(margins + reserved).max(1);
        PageBox {
            width_emu: u64::from(self.text_width_twips(orientation)) * EMU_PER_TWIP,
            height_emu: u64::from(height) * EMU_PER_TWIP,
        }
    }

    /// DXP-FR-LQWZ: the page of a diagram in a paragraph indented by
    /// `indent_twips`. With the switch off, it is portrait. With it on, a
    /// diagram goes on a landscape page when the portrait page shows it at
    /// less than `landscape_below` of its natural size and the landscape page
    /// shows it at a strictly larger scale.
    pub fn diagram_orientation(&self, picture: &Picture, indent_twips: u32) -> Orientation {
        if !self.landscape {
            return Orientation::Portrait;
        }
        let portrait_box = self.page_box(Orientation::Portrait).indented(indent_twips);
        let portrait = fit_scale(picture, portrait_box);
        if portrait >= self.landscape_below {
            return Orientation::Portrait;
        }
        let landscape_box = self.page_box(Orientation::Landscape).indented(indent_twips);
        if fit_scale(picture, landscape_box) > portrait {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        }
    }

    /// DXP-FR-PGMR, DXP-FR-LQWZ: the section properties of a page of
    /// `orientation`, with the margins of the setup.
    pub fn section(&self, orientation: Orientation) -> SectionProperty {
        let (w, h) = page_twips(orientation);
        let orient = match orientation {
            Orientation::Portrait => PageOrientationType::Portrait,
            Orientation::Landscape => PageOrientationType::Landscape,
        };
        SectionProperty::new()
            .page_size(PageSize::new().size(w, h).orient(orient))
            .page_margin(self.page_margin())
    }

    /// DXP-FR-PGMR: the margins of the setup, with the header and footer
    /// distances of the default `docx-rs` page.
    pub fn page_margin(&self) -> PageMargin {
        let side = |twips: u32| i32::try_from(twips).unwrap_or(i32::MAX);
        PageMargin::new()
            .top(side(self.margins.top))
            .bottom(side(self.margins.bottom))
            .left(side(self.margins.left))
            .right(side(self.margins.right))
            .header(HEADER_TWIPS)
            .footer(FOOTER_TWIPS)
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
