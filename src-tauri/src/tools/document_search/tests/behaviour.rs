//! What the tool ranks and drops (SDT-FR-BKAW, SDT-FR-BTOJ, SDT-FR-EMCC,
//! SDT-FR-ONOF, SDT-FR-PJKE, SDT-FR-YEPA, SDT-FR-AMNB, SDT-FR-NZEH).

use super::*;

// SDT-FR-BKAW: only the `documents` index is searched. A project file holding the
// query terms is not returned.
#[test]
fn only_selected_documents_are_searched() {
    let fixture = DocFixture::new();
    fixture.write("refs/a.md", "The axolotl regenerates limbs.");
    fixture.select(SourceKind::Folder, "refs");
    // A specification of the project that matches the same query.
    let project = fixture.fixture.app.state::<crate::bm25_index::Bm25Indexer>().root().unwrap();
    let spec_dir = project.join("specifications");
    std::fs::create_dir_all(&spec_dir).unwrap();
    std::fs::write(spec_dir.join("AXO-axolotl.md"), "# Axolotl\n\nThe axolotl regenerates limbs.").unwrap();
    fixture.fixture.reindex();

    let matches = search(&fixture, "axolotl regenerates", None);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "a.md");
}

// SDT-FR-BTOJ: the default is five documents, and the limit counts documents.
#[test]
fn the_limit_counts_documents() {
    let fixture = DocFixture::new();
    for i in 0..8 {
        fixture.write(&format!("refs/doc{i}.md"), &format!("Lemur {i} lives on Madagascar island"));
    }
    fixture.select(SourceKind::Folder, "refs");
    assert_eq!(search(&fixture, "lemur madagascar", None).len(), 5);
    assert_eq!(search(&fixture, "lemur madagascar", Some(2)).len(), 2);
    assert_eq!(search(&fixture, "lemur madagascar", Some(0)).len(), 1);
    assert_eq!(search(&fixture, "lemur madagascar", Some(1000)).len(), 8);
}

// SDT-FR-EMCC: a document that holds several chunks appears once, ordered by
// descending score, and the best chunk is the one whose passage is returned.
#[test]
fn a_document_appears_once_with_its_best_passage() {
    let fixture = DocFixture::new();
    let sections: String = (0..6)
        .map(|i| format!("# Section {i}\n\nokapi stripes appear in part {i}\n\n"))
        .collect();
    fixture.write("refs/long.md", &format!("{sections}# Best\n\nokapi okapi okapi okapi okapi"));
    fixture.write("refs/other.md", "A single okapi mention.");
    fixture.select(SourceKind::Folder, "refs");

    let matches = search(&fixture, "okapi", Some(5));
    assert_eq!(matches.len(), 2, "two documents, not eight chunks");
    let ids: std::collections::HashSet<&String> = matches.iter().map(|m| &m.id).collect();
    assert_eq!(ids.len(), 2);
    assert!(matches[0].score >= matches[1].score, "descending score");
    let long = matches.iter().find(|m| m.name == "long.md").unwrap();
    assert!(long.snippet.contains("okapi okapi okapi"), "the best chunk: {}", long.snippet);
}

// SDT-FR-ONOF: a hit for a document that left the collection or became
// unavailable is omitted without an error.
#[test]
fn documents_that_left_or_became_unavailable_are_omitted() {
    let fixture = DocFixture::new();
    let gone = fixture.write("refs/gone.md", "Pangolin scales are keratin.");
    fixture.write("refs/kept.md", "Pangolin diet is ants.");
    fixture.write("elsewhere/leaving.md", "Pangolin habitat is forest.");
    fixture.select(SourceKind::Folder, "refs");
    fixture.select(SourceKind::File, "elsewhere/leaving.md");
    assert_eq!(search(&fixture, "pangolin", None).len(), 3);

    // The file disappears and the collection refreshes, but the index has not
    // yet had its pass, so its chunks still answer.
    std::fs::remove_file(&gone).unwrap();
    let fs = fixture.fixture.app.state::<crate::fs::FsAccessState>();
    fixture.collection().refresh(&fs, &fixture.fixture.handle(), &|_| {});
    let names: Vec<String> = search(&fixture, "pangolin", None).into_iter().map(|m| m.name).collect();
    assert!(!names.contains(&"gone.md".to_string()));
    assert_eq!(names.len(), 2);

    // A selected file that is deleted stays in the collection as an unavailable
    // document, and the index has not yet had its pass.
    std::fs::remove_file(fixture.root.join("elsewhere/leaving.md")).unwrap();
    fixture.collection().refresh(&fs, &fixture.fixture.handle(), &|_| {});
    let snapshot = fixture.collection().snapshot().unwrap();
    assert!(snapshot
        .documents
        .iter()
        .any(|d| d.name == "leaving.md" && d.status == crate::documents::Availability::Unavailable));
    let names: Vec<String> = search(&fixture, "pangolin", None).into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["kept.md"]);
}

// SDT-FR-PJKE: the snippet is cut at a character boundary to at most 600
// characters with an ellipsis, and a PDF's snippet is its extracted text.
#[test]
fn a_snippet_is_bounded_and_a_pdf_snippet_is_extracted_text() {
    let long = "tapir ".repeat(120);
    assert_eq!(snippet_of("short"), "short");
    let cut = snippet_of(&long);
    assert_eq!(cut.chars().count(), SNIPPET_LIMIT);
    assert!(cut.ends_with('…'));
    // A multibyte text is cut on a character boundary and never panics.
    let multibyte = "é".repeat(2000);
    let cut = snippet_of(&multibyte);
    assert_eq!(cut.chars().count(), SNIPPET_LIMIT);
    assert!(cut.ends_with('…'));

    let fixture = DocFixture::new();
    fixture.write("refs/paper.pdf", "Quokka smiles are famous worldwide.");
    fixture.select(SourceKind::Folder, "refs");
    let matches = search(&fixture, "quokka smiles", None);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].format, "pdf");
    assert_eq!(matches[0].snippet, "Quokka smiles are famous worldwide.");

    fixture.write("refs/big.md", &long.repeat(5));
    fixture.sync();
    let matches = search(&fixture, "tapir", None);
    assert!(matches[0].snippet.chars().count() <= SNIPPET_LIMIT);
}

// SDT-FR-YEPA: no match, and a project with no selected document, are successes
// with an empty list.
#[test]
fn nothing_matching_is_an_empty_success() {
    let fixture = DocFixture::new();
    assert!(search(&fixture, "anything at all", None).is_empty());
    fixture.write("refs/a.md", "Dugong grazes on seagrass.");
    fixture.select(SourceKind::Folder, "refs");
    assert!(search(&fixture, "completely unrelated zeppelin", None).is_empty());
}

// SDT-FR-AMNB: the call resolves without waiting, and a change to the collection
// reaches it on the next pass with nothing held from the call before.
#[test]
fn the_call_never_waits_and_follows_the_collection() {
    let fixture = DocFixture::new();
    let file = fixture.write("refs/a.md", "Manatee grazing habits.");
    fixture.select(SourceKind::Folder, "refs");
    assert_eq!(search(&fixture, "manatee", None).len(), 1);
    std::fs::write(&file, "Dolphin echolocation habits.").unwrap();
    fixture.sync();
    assert!(search(&fixture, "manatee", None).is_empty());
    assert_eq!(search(&fixture, "dolphin", None).len(), 1);
}

// SDT-FR-NZEH: the tool changes no document, no source, and no index.
#[test]
fn the_tool_is_read_only() {
    let fixture = DocFixture::new();
    let file = fixture.write("refs/a.md", "Wallaby jumps high.");
    fixture.select(SourceKind::Folder, "refs");
    let before = fixture.collection().snapshot().unwrap();
    let sources = fixture.collection().stored_sources();
    let indexed = fixture
        .fixture
        .app
        .state::<crate::bm25_index::Bm25Indexer>()
        .snapshot()
        .chunk_count(crate::bm25_index::IndexId::Documents);
    search(&fixture, "wallaby", None);
    assert_eq!(fixture.collection().snapshot().unwrap(), before);
    assert_eq!(fixture.collection().stored_sources(), sources);
    assert_eq!(
        fixture
            .fixture
            .app
            .state::<crate::bm25_index::Bm25Indexer>()
            .snapshot()
            .chunk_count(crate::bm25_index::IndexId::Documents),
        indexed
    );
    assert_eq!(std::fs::read_to_string(file).unwrap(), "Wallaby jumps high.");
}
