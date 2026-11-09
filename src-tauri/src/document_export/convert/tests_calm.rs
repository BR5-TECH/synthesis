//! Tests of the CALM pictures of an export (`DXP-document-export.md`
//! DXP-FR-WGLT, DXP-FR-TQCQ, DXP-FR-VMPP, DXP-FR-DMUH, DXP-FR-INPH).

use base64::Engine;

use super::calm_raster::{rasterise_svg, MAX_SVG_BYTES};
use super::diagrams::{calm_sources, png_size, DiagramFailure, FailedDiagram};
use super::model::{parse, Block, DiagramKind};
use super::tests::{document_xml, png_files_in, run, write_test_file};
use super::{Format, Stage};
use crate::document_export::commands::{diagram_renderer, CalmPicture};

/// A plain SVG of `width` by `height` with one filled box and one text.
fn svg(width: u32, height: u32, body: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{body}</svg>"##
    )
}

fn box_svg() -> String {
    svg(
        100,
        50,
        r##"<rect x="10" y="10" width="40" height="20" fill="#336699"/><text x="55" y="30" font-family="Liberation Sans" font-size="12">API</text>"##,
    )
}

/// True when every pixel of `png` is white.
fn all_white(png: &[u8]) -> bool {
    let image = image::load_from_memory(png).expect("decodes").to_rgba8();
    image.pixels().all(|p| p.0 == [255, 255, 255, 255])
}

fn mermaid_never(_: &str) -> Result<Vec<u8>, DiagramFailure> {
    Err(DiagramFailure::Invalid)
}

/// A small red PNG.
fn red_png() -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

// DXP-FR-WGLT, DXP-FR-INPH: a `calm` fence, case ignored, is a CALM diagram,
// and Mermaid and CALM fences count together in document order.
#[test]
fn calm_and_mermaid_fences_are_diagrams_counted_together() {
    let doc = parse("```mermaid\ngraph TD; A-->B\n```\n\n```CALM\n{\"nodes\": []}\n```\n\n```mermaid\ngraph TD; C-->D\n```\n");
    let kinds: Vec<(usize, DiagramKind)> = doc
        .blocks
        .iter()
        .filter_map(|top| match &top.block {
            Block::Diagram { index, kind, .. } => Some((*index, *kind)),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![(1, DiagramKind::Mermaid), (2, DiagramKind::Calm), (3, DiagramKind::Mermaid)]
    );
    assert_eq!(doc.diagram_count(), 3);
}

// DXP-FR-TQCQ: the CALM sources, each one time, in the order of the first
// fence, also from a list item and a quote. Mermaid and other code is left out.
#[test]
fn calm_sources_are_distinct_and_in_document_order() {
    let text = "```calm\n{\"b\": 1}\n```\n\n- item\n\n  ```calm\n  {\"a\": 2}\n  ```\n\n> ```calm\n> {\"b\": 1}\n> ```\n\n> ```calm\n> {\"c\": 3}\n> ```\n\n```mermaid\ngraph TD; A-->B\n```\n\n```json\n{\"d\": 4}\n```\n";
    assert_eq!(calm_sources(&parse(text)), vec!["{\"b\": 1}", "{\"a\": 2}", "{\"c\": 3}"]);
    assert!(calm_sources(&parse("# No fences\n")).is_empty());
}

// DXP-FR-WGLT, DXP-FR-DMUH: the raster is a PNG at twice the size of the SVG,
// and it draws the shapes and the text.
#[test]
fn an_svg_rasterises_at_twice_its_size() {
    let png = rasterise_svg(&box_svg()).expect("rasterises");
    assert_eq!(png_size(&png), Some((200, 100)));
    assert!(!all_white(&png), "the box and the text draw");
}

/// One line of text in `family`.
fn text_svg(family: &str) -> String {
    let body =
        format!(r##"<text x="5" y="25" font-family="{family}" font-size="14">Gateway 42</text>"##);
    svg(160, 40, &body)
}

// DXP-FR-DMUH: text in a family that is not bundled draws in the bundled sans
// face, never in a host face: its raster is the raster of Liberation Sans.
// A monospace family draws in Liberation Mono.
#[test]
fn text_draws_in_the_bundled_faces_alone() {
    let bundled = rasterise_svg(&text_svg("Liberation Sans")).expect("rasterises");
    assert!(!all_white(&bundled), "the text draws");
    for family in ["Inter, system-ui", "Helvetica", "Arial", "sans-serif", "Times New Roman"] {
        assert_eq!(rasterise_svg(&text_svg(family)).unwrap(), bundled, "{family}");
    }
    let mono = rasterise_svg(&text_svg("Liberation Mono")).expect("rasterises");
    assert_ne!(mono, bundled, "the mono face is a different face");
    for family in ["monospace", "Menlo, monospace"] {
        assert_eq!(rasterise_svg(&text_svg(family)).unwrap(), mono, "{family}");
    }
}

// DXP-FR-DMUH: an `image` element, an `feImage` filter, and a `use` element
// draw nothing, whether they name a file, a relative path, or a remote URL,
// or hold a data URL. A style sheet loads no other sheet and no font.
#[test]
fn an_image_element_draws_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("red.png");
    write_test_file(&file, &red_png());
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(red_png())
    );
    let hrefs = [
        file.to_string_lossy().into_owned(),
        format!("file://{}", file.to_string_lossy()),
        "red.png".to_string(),
        "https://example.com/red.png".to_string(),
        data,
    ];
    for href in hrefs {
        let body = format!(
            r#"<image href="{href}" x="0" y="0" width="40" height="40"/><image xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="{href}" x="40" y="0" width="40" height="40"/>"#
        );
        let png = rasterise_svg(&svg(80, 40, &body)).expect("rasterises");
        assert!(all_white(&png), "the image of {href} draws nothing");
        // An `feImage` filter and a `use` of another file draw nothing either.
        let filtered = format!(
            r#"<filter id="f" x="0" y="0" width="1" height="1"><feImage href="{href}"/></filter><rect width="80" height="40" fill="none" filter="url(#f)"/><use href="{href}#shape"/>"#
        );
        let png = rasterise_svg(&svg(80, 40, &filtered)).expect("rasterises");
        assert!(all_white(&png), "the filter or the use of {href} draws nothing");
    }
    // A style sheet that imports another one, or loads a font, has no effect.
    let styled = svg(
        160,
        40,
        &format!(
            r##"<style>@import url("{}"); @font-face {{ font-family: "Remote"; src: url("https://example.com/f.ttf"); }}</style><text x="5" y="25" font-family="Remote" font-size="14">Gateway 42</text>"##,
            file.to_string_lossy()
        ),
    );
    assert_eq!(
        rasterise_svg(&styled).unwrap(),
        rasterise_svg(&text_svg("Liberation Sans")).unwrap(),
        "the text draws in the bundled face"
    );
}

// DXP-FR-DMUH: an empty, malformed, too large, or out-of-bounds SVG does not
// rasterise.
#[test]
fn an_svg_out_of_its_limits_does_not_rasterise() {
    assert_eq!(rasterise_svg(""), Err(DiagramFailure::Invalid));
    assert_eq!(rasterise_svg("<svg"), Err(DiagramFailure::Invalid));
    assert_eq!(rasterise_svg("{\"nodes\": []}"), Err(DiagramFailure::Invalid));
    let padding = " ".repeat(MAX_SVG_BYTES);
    let large = svg(10, 10, &format!("<g>{padding}</g>"));
    assert!(large.len() > MAX_SVG_BYTES);
    assert_eq!(rasterise_svg(&large), Err(DiagramFailure::Invalid));
    assert_eq!(rasterise_svg(&svg(0, 10, "")), Err(DiagramFailure::Invalid));
    assert_eq!(rasterise_svg(&svg(8001, 10, "")), Err(DiagramFailure::Invalid));
    assert_eq!(rasterise_svg(&svg(10, 8001, "")), Err(DiagramFailure::Invalid));
    assert!(rasterise_svg(&svg(8000, 1, "")).is_ok(), "the limit itself rasterises");
}

// DXP-FR-DMUH, DXP-FR-WGLT: a raster is at most 4000 px on its longest side,
// so the largest SVG makes a raster of 4000 by 4000 and not 16000 by 16000.
#[test]
fn a_large_svg_rasterises_at_most_4000_px_on_its_longest_side() {
    let full = r##"<rect width="100%" height="100%" fill="#336699"/>"##;
    let size = |w: u32, h: u32| png_size(&rasterise_svg(&svg(w, h, full)).unwrap());
    assert_eq!(size(8000, 8000), Some((4000, 4000)));
    assert_eq!(size(3000, 100), Some((4000, 133)));
    assert_eq!(size(100, 2000), Some((200, 4000)));
    assert_eq!(size(1999, 100), Some((3998, 200)));
}

// DXP-FR-VMPP: the frontend sends each picture as `{ source, svg }`.
#[test]
fn a_calm_picture_reads_from_its_wire_shape() {
    let picture: CalmPicture =
        serde_json::from_str(r#"{"source":"{}","svg":"<svg/>"}"#).expect("deserialises");
    assert_eq!(picture, CalmPicture { source: "{}".into(), svg: "<svg/>".into() });
}

// DXP-FR-VMPP: a CALM fence finds its picture by its exact source, the first
// entry of a source applies, and a fence with no picture does not render.
// A Mermaid fence goes to the Mermaid renderer.
#[test]
fn a_calm_fence_finds_its_picture_by_its_exact_source() {
    let pictures = vec![
        CalmPicture { source: "{\"a\": 1}".into(), svg: box_svg() },
        CalmPicture { source: "{\"a\": 1}".into(), svg: "<not svg".into() },
        CalmPicture { source: "{\"b\": 2}".into(), svg: "<not svg".into() },
    ];
    let mermaid = |_: &str| Ok(vec![7]);
    let render = diagram_renderer(&pictures, &mermaid);
    assert_eq!(png_size(&render(DiagramKind::Calm, "{\"a\": 1}").unwrap()), Some((200, 100)));
    assert_eq!(render(DiagramKind::Calm, "{\"a\":1}"), Err(DiagramFailure::Invalid));
    assert_eq!(render(DiagramKind::Calm, "{\"b\": 2}"), Err(DiagramFailure::Invalid));
    assert_eq!(render(DiagramKind::Calm, "{\"c\": 3}"), Err(DiagramFailure::Invalid));
    assert_eq!(render(DiagramKind::Mermaid, "{\"a\": 1}"), Ok(vec![7]));
}

// DXP-FR-WGLT, DXP-FR-IOFM, DXP-FR-INPH: in a DOCX, a CALM fence with a
// picture is an image, and one with no picture stays as code with a warning
// whose index counts the Mermaid fence before it. The progress counts the
// Mermaid and the CALM fences together.
#[test]
fn a_docx_shows_a_calm_picture_and_keeps_a_fence_with_no_picture() {
    let dir = tempfile::tempdir().unwrap();
    let pictures = vec![CalmPicture { source: "{\"drawn\": true}".into(), svg: box_svg() }];
    let render = diagram_renderer(&pictures, &mermaid_never);
    let text = "```calm\n{\"drawn\": true}\n```\n\n```mermaid\ngraph TD; A-->B\n```\n\n```calm\n{\"missing\": true}\n```\n";
    let (converted, events) = run(text, Format::Docx, false, &render, dir.path());
    let diagrams: Vec<_> =
        events.iter().filter(|(stage, _, _)| *stage == Stage::Diagrams).collect();
    let expected = [
        (Stage::Diagrams, 0, 3),
        (Stage::Diagrams, 1, 3),
        (Stage::Diagrams, 2, 3),
        (Stage::Diagrams, 3, 3),
    ];
    assert_eq!(diagrams, expected.iter().collect::<Vec<_>>());
    assert_eq!(
        converted.failed_diagrams,
        vec![
            FailedDiagram { index: 2, reason: DiagramFailure::Invalid },
            FailedDiagram { index: 3, reason: DiagramFailure::Invalid },
        ]
    );
    assert_eq!(converted.diagram_count, 3);
    let xml = document_xml(&converted.bytes);
    assert_eq!(xml.matches("<pic:pic").count(), 1, "one picture");
    assert!(!xml.contains("drawn"), "the drawn source is gone");
    assert!(xml.contains("missing"), "the source with no picture stays");
}

// DXP-FR-WGLT: in a PDF, a CALM picture is placed as an image.
#[test]
fn a_pdf_places_a_calm_picture() {
    let dir = tempfile::tempdir().unwrap();
    let pictures = vec![CalmPicture { source: "{\"drawn\": true}".into(), svg: box_svg() }];
    let render = diagram_renderer(&pictures, &mermaid_never);
    let (converted, _) =
        run("```calm\n{\"drawn\": true}\n```\n", Format::Pdf, false, &render, dir.path());
    assert!(converted.failed_diagrams.is_empty());
    assert!(converted.bytes.starts_with(b"%PDF-"));
    assert!(png_files_in(dir.path()) >= 1, "the picture is written for the PDF");
}

/// The print drawings of the CALM templates, as the frontend makes them
/// (`src/components/CalmDiagram/printSvg.fixtures.test.ts`).
const FIXTURES: [(&str, &str); 4] = [
    ("minimal", include_str!("fixtures/calm-minimal.svg")),
    ("three-tier", include_str!("fixtures/calm-three-tier.svg")),
    ("deployed", include_str!("fixtures/calm-deployed.svg")),
    ("flow", include_str!("fixtures/calm-flow.svg")),
];

/// `svg` with each `<text>` element removed.
fn without_text(svg: &str) -> String {
    let mut out = String::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text ") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("</text>").expect("closed") + start + "</text>".len();
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The width and the height that an SVG fixture declares.
fn declared_size(svg: &str) -> (u32, u32) {
    let number = |name: &str| -> u32 {
        let start = svg.find(&format!(" {name}=\"")).expect("declared") + name.len() + 3;
        svg[start..].split('"').next().unwrap().parse().unwrap()
    };
    (number("width"), number("height"))
}

// DXP-FR-WGLT, DXP-FR-DMUH, CAD-FR-QHEJ: the backend rasterises the print
// drawing of each CALM template at twice its size. With
// `CALM_RASTER_OUT=<dir>`, the test also writes each PNG there to look at.
#[test]
fn the_print_drawings_of_the_templates_rasterise() {
    let out = std::env::var_os("CALM_RASTER_OUT").map(std::path::PathBuf::from);
    for (name, svg) in FIXTURES {
        let png = rasterise_svg(svg).unwrap_or_else(|_| panic!("{name} rasterises"));
        let (width, height) = declared_size(svg);
        assert_eq!(png_size(&png), Some((width * 2, height * 2)), "{name}");
        assert!(!all_white(&png), "{name} draws");
        // The text of the drawing draws too: with no text, the raster differs.
        assert_ne!(rasterise_svg(&without_text(svg)).unwrap(), png, "{name} draws its text");
        if let Some(dir) = &out {
            write_test_file(&dir.join(format!("calm-{name}.png")), &png);
        }
    }
    if let Some(dir) = &out {
        // A document with each drawing as a fence, in both formats.
        let pictures: Vec<CalmPicture> = FIXTURES
            .iter()
            .map(|(name, svg)| CalmPicture { source: (*name).into(), svg: (*svg).into() })
            .collect();
        let render = diagram_renderer(&pictures, &mermaid_never);
        let text: String = FIXTURES
            .iter()
            .map(|(name, _)| format!("## {name}\n\nThe architecture:\n\n```calm\n{name}\n```\n\n"))
            .collect();
        let work = tempfile::tempdir().unwrap();
        for format in [Format::Pdf, Format::Docx] {
            let (converted, _) = run(&text, format, false, &render, work.path());
            assert!(converted.failed_diagrams.is_empty());
            write_test_file(
                &dir.join(format!("calm-templates.{}", format.as_str())),
                &converted.bytes,
            );
        }
    }
}
