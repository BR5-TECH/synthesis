//! Tests of the pages of a DOCX export (`DXP-document-export.md`
//! DXP-FR-HKFT, DXP-FR-LQWZ, DXP-FR-RKJC).
//!
//! A test diagram gives its natural size as its source, `<width>x<height>`, so
//! one document can hold diagrams of each size.

use std::time::Duration;

use super::diagrams::{self, png_size, DiagramFailure, MAX_RASTER_PIXELS, MAX_RASTER_SIDE};
use super::docx_page::{self, Orientation, EMU_PER_PX};
use super::model::DiagramKind;
use super::tests::{document_xml, picture, run};
use super::{document_of, docx, Assets, Format, Picture};

/// A diagram renderer that reads the natural size from the source.
fn sized(_: DiagramKind, source: &str) -> Result<Picture, DiagramFailure> {
    let (w, h) = source.trim().split_once('x').ok_or(DiagramFailure::Invalid)?;
    let w: u32 = w.parse().map_err(|_| DiagramFailure::Invalid)?;
    let h: u32 = h.parse().map_err(|_| DiagramFailure::Invalid)?;
    Ok(picture(3, 3, w, h))
}

fn docx_of(text: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let (converted, _) = run(text, Format::Docx, false, &sized, dir.path());
    assert!(converted.failed_diagrams.is_empty());
    document_xml(&converted.bytes)
}

fn fence(size: &str) -> String {
    format!("```mermaid\n{size}\n```\n\n")
}

/// One section of a document: its orientation, its pictures, its margins,
/// and its XML.
#[derive(Debug)]
struct Section {
    landscape: bool,
    pictures: usize,
    margins: String,
    xml: String,
}

/// The value of the attribute `name` in `xml`, after `from`.
fn attribute<'a>(xml: &'a str, from: &str, name: &str) -> &'a str {
    let at = xml.find(from).unwrap_or_else(|| panic!("{from} in {xml}"));
    let rest = &xml[at..];
    let start = rest.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
    rest[start..].split('"').next().unwrap()
}

/// The sections of `xml` in document order. A `w:sectPr` in the properties
/// of a paragraph ends its section with the end of that paragraph. The
/// `w:sectPr` of the body ends the last section.
fn sections(xml: &str) -> Vec<Section> {
    let starts: Vec<usize> = xml.match_indices("<w:sectPr").map(|(i, _)| i).collect();
    let mut sections = Vec::new();
    let mut from = 0;
    for (n, &start) in starts.iter().enumerate() {
        let last = n + 1 == starts.len();
        let properties_end = start + xml[start..].find("</w:sectPr>").unwrap();
        let properties = &xml[start..properties_end];
        if !last {
            // A section that is not the last ends in the properties of a
            // paragraph: no paragraph starts before those properties close.
            let to_ppr = &xml[start..start + xml[start..].find("</w:pPr>").expect("in a pPr")];
            assert!(!to_ppr.contains("<w:p>") && !to_ppr.contains("<w:p "), "{to_ppr}");
        }
        // DXP-FR-LQWZ: every page is A4, and its orientation matches its size.
        let w: u32 = attribute(properties, "<w:pgSz", "w:w").parse().unwrap();
        let h: u32 = attribute(properties, "<w:pgSz", "w:h").parse().unwrap();
        assert!((w, h) == (11_906, 16_838) || (w, h) == (16_838, 11_906), "{w} x {h}");
        let landscape = properties.contains(r#"w:orient="landscape""#);
        assert_eq!(landscape, w > h, "{properties}");
        let margins_at = properties.find("<w:pgMar").expect("margins");
        let margins = properties[margins_at..].split("/>").next().unwrap().to_string();
        let end = if last {
            start
        } else {
            properties_end + xml[properties_end..].find("</w:p>").unwrap()
        };
        let content = &xml[from..end];
        // DXP-FR-HKFT: each picture fits the box of the page it is on.
        let frame = docx_page::page_box(if landscape {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        });
        for (cx, cy) in extents(content) {
            assert!(cx <= frame.width_emu && cy <= frame.height_emu, "{cx} x {cy} in {frame:?}");
        }
        sections.push(Section {
            landscape,
            pictures: content.matches("<pic:pic").count(),
            margins,
            xml: content.to_string(),
        });
        from = end;
    }
    // DXP-FR-LQWZ: the landscape page has the margins of the portrait page.
    assert!(sections.windows(2).all(|s| s[0].margins == s[1].margins), "{sections:?}");
    sections
}

/// The `wp:extent` of each picture, in EMU, in document order.
fn extents(xml: &str) -> Vec<(u64, u64)> {
    let number = |s: &str, name: &str| -> u64 {
        let start = s.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        s[start..].split('"').next().unwrap().parse().unwrap()
    };
    xml.split("<wp:extent ").skip(1).map(|s| (number(s, "cx"), number(s, "cy"))).collect()
}

fn portrait_box() -> docx_page::PageBox {
    docx_page::page_box(Orientation::Portrait)
}

fn landscape_box() -> docx_page::PageBox {
    docx_page::page_box(Orientation::Landscape)
}

// DXP-FR-HKFT: the boxes of the A4 pages with the default margins, less the
// space after the paragraph and one body line.
#[test]
fn the_page_boxes_are_the_usable_a4_areas() {
    assert_eq!(portrait_box().width_emu, 8_504 * 635);
    assert_eq!(portrait_box().height_emu, (16_838 - 1_985 - 1_701 - 180 - 330) * 635);
    assert_eq!(landscape_box().width_emu, 13_436 * 635);
    assert_eq!(landscape_box().height_emu, (11_906 - 1_985 - 1_701 - 180 - 330) * 635);
}

// DXP-FR-HKFT, DXP-FR-RKJC: a picture that fits stays at its natural size and
// is never scaled up. A larger one keeps its aspect ratio and fits both sides.
#[test]
fn a_picture_fits_its_box_and_is_never_scaled_up() {
    let fit = |w: u32, h: u32| docx_page::fit(&picture(1, 1, w, h), portrait_box());
    let px = |n: u64| (n * EMU_PER_PX) as u32;
    assert_eq!(fit(40, 20), (px(40), px(20)));
    assert_eq!(fit(1, 1), (px(1), px(1)));
    let frame = portrait_box();
    let (w, h) = fit(1134, 100);
    assert!(u64::from(w) <= frame.width_emu && u64::from(w) + 2 >= frame.width_emu);
    assert!((f64::from(h) / f64::from(w) - 100.0 / 1134.0).abs() < 1e-4, "keeps its ratio");
    let (w, h) = fit(100, 5000);
    assert!(u64::from(h) <= frame.height_emu && u64::from(h) + 2 >= frame.height_emu);
    assert!((f64::from(w) / f64::from(h) - 100.0 / 5000.0).abs() < 1e-4, "keeps its ratio");
}

// DXP-FR-LQWZ: a diagram goes on a landscape page only when the portrait page
// shows it at less than 75% of its natural size and the landscape page shows
// it larger.
#[test]
fn a_diagram_goes_on_a_landscape_page_only_when_it_shows_larger() {
    let page = |w: u32, h: u32| docx_page::diagram_orientation(&picture(1, 1, w, h), 0);
    // The portrait text width is 8504 twips, about 566.9 px, so 75% of the
    // natural size fits for a width of up to about 755.9 px.
    assert_eq!(page(566, 100), Orientation::Portrait, "as wide as the text");
    assert_eq!(page(600, 150), Orientation::Portrait, "a little wider than the text");
    assert_eq!(page(755, 100), Orientation::Portrait, "shown at just over 75%");
    assert_eq!(page(756, 100), Orientation::Landscape, "shown at just under 75%");
    assert_eq!(page(1200, 300), Orientation::Landscape);
    // Width limits the portrait page and height limits the landscape page.
    assert_eq!(page(1000, 850), Orientation::Landscape, "landscape shows 0.605 > 0.567");
    assert_eq!(page(1000, 950), Orientation::Portrait, "landscape shows 0.541 < 0.567");
    // 75% exactly stays portrait: 724 px beside an indent of 359 twips.
    let at = |w: u32, indent: u32| docx_page::diagram_orientation(&picture(1, 1, w, 100), indent);
    assert_eq!(at(724, 359), Orientation::Portrait, "exactly 75%");
    // The indent counts: beside 360 twips, 723 px shows at 75.09% and 724 px at 74.99%.
    assert_eq!(at(723, 360), Orientation::Portrait);
    assert_eq!(at(724, 360), Orientation::Landscape);
    assert_eq!(page(1500, 3000), Orientation::Portrait, "landscape shows it smaller");
    assert_eq!(page(300, 2000), Orientation::Portrait, "narrow and tall");
}

// DXP-FR-LQWZ, DXP-FR-HKFT: a diagram a little wider than the text stays on
// the portrait page, scaled down to the text width.
#[test]
fn a_diagram_a_little_wider_than_the_text_stays_on_the_portrait_page() {
    let xml = docx_of(&format!("Intro\n\n{}After\n", fence("600x150")));
    let all = sections(&xml);
    assert_eq!(all.len(), 1, "{all:?}");
    assert!(!all[0].landscape);
    let (cx, _) = extents(&xml)[0];
    assert!(cx <= portrait_box().width_emu && cx + 2 >= portrait_box().width_emu, "{cx}");
}

// DXP-FR-LQWZ, DXP-FR-HKFT, DXP-FR-RKJC: a narrow diagram stays on the
// portrait page at its natural size, and the document has one section only.
#[test]
fn a_narrow_diagram_stays_inline_at_its_natural_size() {
    let xml = docx_of(&format!("Intro\n\n{}After\n", fence("200x100")));
    let all = sections(&xml);
    assert_eq!(all.len(), 1, "{all:?}");
    assert!(!all[0].landscape);
    assert_eq!(extents(&xml), vec![(200 * EMU_PER_PX, 100 * EMU_PER_PX)]);
}

// DXP-FR-LQWZ, DXP-FR-HKFT: a wide diagram is in a landscape section of its
// own between two portrait sections, scaled down to the landscape text width.
#[test]
fn a_wide_diagram_is_on_a_landscape_page_between_portrait_pages() {
    let xml = docx_of(&format!("Before\n\n{}After\n", fence("1200x300")));
    let all = sections(&xml);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(!all[0].landscape && all[0].xml.contains("Before") && all[0].pictures == 0);
    assert!(all[1].landscape && all[1].pictures == 1 && !all[1].xml.contains("After"));
    assert!(!all[2].landscape && all[2].xml.contains("After"));
    assert_eq!(xml.matches(r#"w:w="16838" w:h="11906""#).count(), 1);
    let (cx, cy) = extents(&xml)[0];
    assert!(cx <= landscape_box().width_emu && cx + 2 >= landscape_box().width_emu);
    assert!((cy as f64 / cx as f64 - 0.25).abs() < 1e-4, "keeps its ratio");
}

// DXP-FR-LQWZ: wide diagrams with no other block between them share one
// landscape section.
#[test]
fn wide_diagrams_next_to_each_other_share_one_landscape_section() {
    let xml = docx_of(&format!("Before\n\n{}{}After\n", fence("1200x300"), fence("900x200")));
    let all = sections(&xml);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all[1].landscape);
    assert_eq!(all[1].pictures, 2);
}

// DXP-FR-LQWZ: wide diagrams with a block between them are in two landscape
// sections, and the block between them is on a portrait page.
#[test]
fn a_block_between_wide_diagrams_is_on_a_portrait_page() {
    let xml = docx_of(&format!("{}Between\n\n{}", fence("1200x300"), fence("900x200")));
    let all = sections(&xml);
    let orientations: Vec<bool> = all.iter().map(|s| s.landscape).collect();
    assert_eq!(orientations, vec![true, false, true], "{all:?}");
    assert!(all[1].xml.contains("Between"));
}

// DXP-FR-LQWZ: a wide diagram that is the first block opens no portrait page
// before it, and one that is the last block makes the body section landscape.
#[test]
fn a_wide_diagram_first_or_last_makes_no_empty_portrait_page() {
    let xml = docx_of(&format!("{}After\n", fence("1200x300")));
    let all = sections(&xml);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[0].landscape && all[0].pictures == 1);
    assert!(!all[1].landscape && all[1].xml.contains("After"));

    let xml = docx_of(&format!("Before\n\n{}", fence("1200x300")));
    let all = sections(&xml);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(!all[0].landscape && all[0].xml.contains("Before"));
    assert!(all[1].landscape && all[1].pictures == 1, "the body section is landscape");

    let xml = docx_of(&fence("1200x300"));
    let all = sections(&xml);
    assert_eq!(all.len(), 1, "{all:?}");
    assert!(all[0].landscape);
}

// DXP-FR-LQWZ: a wide diagram in a list item or a quote goes on a landscape
// page by the same rule.
#[test]
fn a_wide_diagram_in_a_list_or_a_quote_goes_on_a_landscape_page() {
    let text =
        "- item\n\n  ```mermaid\n  1200x300\n  ```\n\n> ```mermaid\n> 1000x300\n> ```\n\nAfter\n";
    let all = sections(&docx_of(text));
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(!all[0].landscape && all[0].xml.contains("item"));
    assert!(all[1].landscape && all[1].pictures == 2);
    assert!(!all[2].landscape && all[2].xml.contains("After"));
}

// DXP-FR-HKFT, DXP-FR-LQWZ: an indented diagram fits the width that is left
// beside its indent. A diagram of 740 px shows at more than 75% on the
// portrait page, and in a quote, 360 twips narrower, at less: there it goes on
// a landscape page.
#[test]
fn an_indented_diagram_fits_the_width_beside_its_indent() {
    let indented = portrait_box().indented(360);
    assert_eq!(indented.width_emu, (8_504 - 360) * 635);
    assert_eq!(docx_page::diagram_orientation(&picture(1, 1, 740, 100), 0), Orientation::Portrait);
    let xml = docx_of("> ```mermaid\n> 740x100\n> ```\n\nAfter\n");
    let all = sections(&xml);
    assert!(all[0].landscape && all[0].pictures == 1, "{all:?}");
    // In a quote in a list item the indent is 720 twips: 700 px shows at
    // 74.1% there, and at 77.6% beside 360 twips.
    assert_eq!(
        docx_page::diagram_orientation(&picture(1, 1, 700, 100), 360),
        Orientation::Portrait
    );
    let all = sections(&docx_of("- item\n\n  > ```mermaid\n  > 700x100\n  > ```\n\nAfter\n"));
    assert!(all.iter().any(|s| s.landscape && s.pictures == 1), "{all:?}");
    let xml = docx_of("> ```mermaid\n> 4000x300\n> ```\n");
    let (cx, _) = extents(&xml)[0];
    let frame = landscape_box().indented(360);
    assert!(cx <= frame.width_emu && cx + 2 >= frame.width_emu, "{cx}");
}

// DXP-FR-LQWZ, DXP-FR-SEDX, DXP-FR-RWWA: a V1 break next to a landscape
// section makes no page of its own, because the section starts a new page.
// A V1 break between portrait blocks stays a page break.
#[test]
fn a_page_break_next_to_a_landscape_section_adds_no_empty_page() {
    let text = format!(
        "Before\n\n<!-- % -->\n\n{}<!-- % -->\n\nAfter\n\n<!-- % -->\n\nEnd\n",
        fence("1200x300")
    );
    let xml = docx_of(&text);
    let all = sections(&xml);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all[1].landscape && all[1].pictures == 1);
    assert_eq!(xml.matches(r#"w:type="page""#).count(), 1, "only the portrait break stays");
    assert!(all[2].xml.contains("After") && all[2].xml.contains("End"));
}

// DXP-FR-HKFT: a tall diagram is scaled down to the usable height of the
// portrait page, and a tall and wide one stays portrait.
#[test]
fn a_tall_diagram_fits_the_usable_height_of_the_page() {
    let xml = docx_of(&format!("{}{}", fence("300x2000"), fence("1500x3000")));
    assert_eq!(sections(&xml).len(), 1, "both stay portrait");
    let frame = portrait_box();
    for (cx, cy) in extents(&xml) {
        assert!(cy <= frame.height_emu && cy + 2 >= frame.height_emu, "{cx} x {cy}");
        assert!(cx <= frame.width_emu);
    }
}

// DXP-FR-HKFT: a project image that is taller than the page is scaled down to
// the usable height of the portrait page.
#[test]
fn a_tall_image_fits_the_usable_height_of_the_page() {
    let doc = document_of("![tall](tall.png)\n", false);
    let mut assets = Assets::default();
    assets.images.insert("tall.png".into(), picture(4, 30, 100, 3000));
    let xml = document_xml(&docx::write(&doc, &assets).expect("writes"));
    let (cx, cy) = extents(&xml)[0];
    assert!(cy <= portrait_box().height_emu && cy + 2 >= portrait_box().height_emu);
    assert!((cx as f64 / cy as f64 - 100.0 / 3000.0).abs() < 1e-4);
}

// DXP-FR-RKJC, DXP-FR-LQWZ: a real wide sequence diagram renders in the raster
// limits, keeps its layout size as its natural size, and goes on a landscape
// page.
#[test]
fn merman_renders_a_wide_sequence_diagram_in_the_raster_limits() {
    let names = ["Alice", "Bob", "Carol", "Dave", "Erin", "Frank", "Grace", "Heidi", "Ivan"];
    let mut source = String::from("sequenceDiagram\n");
    for pair in names.windows(2) {
        source.push_str(&format!(
            "  {}->>{}: request from {} to {}\n",
            pair[0], pair[1], pair[0], pair[1]
        ));
    }
    // A long deadline: a debug build on a slow runner renders much slower.
    let picture = diagrams::render_png_within(&source, Duration::from_secs(50)).expect("renders");
    let (pw, ph) = png_size(&picture.png).unwrap();
    assert!(pw.max(ph) <= MAX_RASTER_SIDE);
    assert!(u64::from(pw) * u64::from(ph) <= MAX_RASTER_PIXELS);
    let limited = pw == MAX_RASTER_SIDE || u64::from(pw) * u64::from(ph) > 5_900_000;
    assert!(limited, "the diagram is large enough to reach a raster limit: {pw} x {ph}");
    assert!(picture.width * 3 > pw + 30, "the natural size is not the raster divided by 3");
    let ratio = (pw as f64 / ph as f64) / (picture.width as f64 / picture.height as f64);
    assert!((ratio - 1.0).abs() < 0.02, "the natural size has the ratio of the raster");
    assert_eq!(docx_page::diagram_orientation(&picture, 0), Orientation::Landscape);
}

// DXP-FR-RKJC: a small real diagram renders at three times its natural size.
#[test]
fn merman_renders_a_small_diagram_at_three_times_its_size() {
    let picture = diagrams::render_png_within("flowchart LR\n  A --> B", Duration::from_secs(50))
        .expect("renders");
    let (pw, ph) = png_size(&picture.png).unwrap();
    assert!(pw.abs_diff(picture.width * 3) <= 3, "{pw} for {}", picture.width);
    assert!(ph.abs_diff(picture.height * 3) <= 3, "{ph} for {}", picture.height);
}

// DXP-FR-LQWZ, DXP-FR-SEDX: a V1 break between two wide diagrams puts each
// one in a landscape section of its own, and the first one stays landscape.
#[test]
fn a_page_break_between_wide_diagrams_makes_two_landscape_sections() {
    let text = format!("{}<!-- % -->\n\n{}After\n", fence("1200x300"), fence("900x200"));
    let xml = docx_of(&text);
    let all = sections(&xml);
    let shape: Vec<(bool, usize)> = all.iter().map(|s| (s.landscape, s.pictures)).collect();
    assert_eq!(shape, vec![(true, 1), (true, 1), (false, 0)], "{all:?}");
    assert_eq!(xml.matches(r#"w:type="page""#).count(), 0);
}

// DXP-FR-LQWZ: a code block before and a table after a wide diagram stay on
// portrait pages, and no page is empty.
#[test]
fn a_table_next_to_a_wide_diagram_stays_on_a_portrait_page() {
    let text =
        format!("```text\ncode\n```\n\n{}| a | b |\n|---|---|\n| 1 | 2 |\n", fence("1200x300"));
    let all = sections(&docx_of(&text));
    let shape: Vec<(bool, usize)> = all.iter().map(|s| (s.landscape, s.pictures)).collect();
    assert_eq!(shape, vec![(false, 0), (true, 1), (false, 0)], "{all:?}");
    assert!(all[0].xml.contains("code") && all[0].xml.contains("<w:tbl>"));
    assert!(all[2].xml.contains("<w:tbl>"));
}

// DXP-FR-LQWZ: text after a wide diagram in the same list item is on a
// portrait page again.
#[test]
fn list_text_after_a_wide_diagram_is_on_a_portrait_page() {
    let text = "- first\n\n  ```mermaid\n  1200x300\n  ```\n\n  more text\n- second\n";
    let all = sections(&docx_of(text));
    let shape: Vec<(bool, usize)> = all.iter().map(|s| (s.landscape, s.pictures)).collect();
    assert_eq!(shape, vec![(false, 0), (true, 1), (false, 0)], "{all:?}");
    assert!(all[2].xml.contains("more text") && all[2].xml.contains("second"));
}

// DXP-FR-SEDX, DXP-FR-LQWZ: after a landscape section, the first of two V1
// breaks is the section end, and the second one stays a page break.
#[test]
fn two_page_breaks_after_a_wide_diagram_keep_one_page_break() {
    let text = format!("{}<!-- % -->\n\n<!-- % -->\n\nAfter\n", fence("1200x300"));
    let xml = docx_of(&text);
    assert_eq!(sections(&xml).len(), 2);
    assert_eq!(xml.matches(r#"w:type="page""#).count(), 1);
}
