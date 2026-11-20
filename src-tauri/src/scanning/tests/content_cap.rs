//! The capped content read of the classifier (ASC-FR-HWQJ, ASC-FR-04).
//!
//! One part of `../tests/mod.rs`.

use super::*;

fn facts(body: &[u8]) -> ContentFacts {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("n.md"), body).unwrap();
    let root = crate::fs::RootFs::for_root(tmp.path());
    markdown_facts(&root)("n.md")
}

const FRONT: &str = "---\ntype: scenario\n---\n";

// ASC-FR-HWQJ: frontmatter that closes inside the cap is read, however large
// the file is.
#[test]
fn frontmatter_inside_the_cap_is_read_in_a_large_file() {
    let mut body = FRONT.as_bytes().to_vec();
    body.extend(std::iter::repeat_n(b'x', CONTENT_READ_CAP * 2));
    assert_eq!(facts(&body).artifact_type, Some(ArtifactType::Scenario));
}

// ASC-FR-HWQJ: frontmatter that does not close inside the cap is absent.
#[test]
fn frontmatter_past_the_cap_is_absent() {
    let mut body = b"---\n".to_vec();
    body.extend(std::iter::repeat_n(b'y', CONTENT_READ_CAP));
    body.extend(b"\ntype: scenario\n---\n");
    assert_eq!(facts(&body), ContentFacts::default());
}

// ASC-FR-HWQJ: a closing line that the cap cuts from a longer line closes
// nothing.
#[test]
fn a_closing_line_cut_by_the_cap_closes_nothing() {
    let mut body = b"---\ntype: scenario\n".to_vec();
    let pad = CONTENT_READ_CAP - body.len() - 4;
    body.extend(std::iter::repeat_n(b'z', pad));
    body.extend(b"\n---");
    assert_eq!(body.len(), CONTENT_READ_CAP);
    body.extend(b"--- not a close\n");
    assert_eq!(facts(&body), ContentFacts::default());
}

// ASC-FR-HWQJ: a file of exactly the cap is read whole.
#[test]
fn a_file_of_exactly_the_cap_is_complete() {
    let mut body = FRONT.as_bytes().to_vec();
    body.extend(std::iter::repeat_n(b'x', CONTENT_READ_CAP - FRONT.len()));
    assert_eq!(body.len(), CONTENT_READ_CAP);
    assert_eq!(facts(&body).artifact_type, Some(ArtifactType::Scenario));
}

// ASC-FR-HWQJ: a character that the cap cuts in two is dropped, and a file
// that is not UTF-8 has no facts.
#[test]
fn utf8_is_respected_at_the_cap() {
    let mut body = FRONT.as_bytes().to_vec();
    body.extend(std::iter::repeat_n(b'x', CONTENT_READ_CAP - FRONT.len() - 2));
    body.extend("😀".as_bytes());
    assert_eq!(facts(&body).artifact_type, Some(ArtifactType::Scenario));
    let mut bad = FRONT.as_bytes().to_vec();
    bad.push(0xFF);
    assert_eq!(facts(&bad), ContentFacts::default());
}
