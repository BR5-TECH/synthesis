//! The typography of an export, the same in the `pdf` and the `docx` format
//! (DXP-FR-ODVI, DXP-FR-RBAQ).
//!
//! The values come from the default Rich Markdown role of the editor (15 px
//! body, heading scale 28 / 22 / 18 / 16.5 / 15 px, 12 px after a paragraph),
//! set for print at an 11 pt body. All values are in points.

/// The body text size.
pub const BODY_PT: f32 = 11.0;
/// The line height of body text, as a multiple of the font size.
pub const BODY_LINE_HEIGHT: f32 = 1.5;
/// The space after a paragraph.
pub const PARAGRAPH_AFTER_PT: f32 = 9.0;

/// The look of one heading level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadingLook {
    pub size_pt: f32,
    pub before_pt: f32,
    pub after_pt: f32,
}

/// H1 to H6. Every heading is bold.
pub const HEADINGS: [HeadingLook; 6] = [
    HeadingLook { size_pt: 20.5, before_pt: 0.0, after_pt: 12.0 },
    HeadingLook { size_pt: 16.0, before_pt: 18.0, after_pt: 9.0 },
    HeadingLook { size_pt: 13.0, before_pt: 15.0, after_pt: 6.0 },
    HeadingLook { size_pt: 12.0, before_pt: 12.0, after_pt: 6.0 },
    HeadingLook { size_pt: 11.0, before_pt: 12.0, after_pt: 6.0 },
    HeadingLook { size_pt: 11.0, before_pt: 12.0, after_pt: 6.0 },
];

/// The heading line height, as a multiple of the font size.
pub const HEADING_LINE_HEIGHT: f32 = 1.2;

/// The space between two items of a list.
pub const LIST_ITEM_GAP_PT: f32 = 3.0;
/// The space after a list, a code block, a table, and a quote.
pub const BLOCK_AFTER_PT: f32 = 9.0;
/// The space above and below a thematic break.
pub const RULE_SPACE_PT: f32 = 18.0;

/// The size of inline code and code blocks.
pub const CODE_PT: f32 = 10.0;
/// The line height inside a code block, as a multiple of the font size.
pub const CODE_LINE_HEIGHT: f32 = 1.4;

/// The look of the heading of `level`, 1 to 6. A level outside that range
/// takes the look of H6.
pub fn heading(level: u8) -> HeadingLook {
    HEADINGS[usize::from(level.clamp(1, 6)) - 1]
}

/// `pt` in the half-points of a `docx` font size.
pub fn half_points(pt: f32) -> usize {
    (pt * 2.0).round() as usize
}

/// `pt` in the twentieths of a point of `docx` spacing.
pub fn twips(pt: f32) -> u32 {
    (pt * 20.0).round() as u32
}

/// A line height multiple in the 240ths of a `docx` "auto" line rule.
pub fn auto_line(multiple: f32) -> i32 {
    (multiple * 240.0).round() as i32
}
