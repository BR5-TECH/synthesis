//! The media of a surface (AUI-FR-GUUJ, AUI-FR-JUDL, AUI-FR-YXMD).

use base64::Engine as _;
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::agent_conversations::ConversationOrigin;
use crate::agent_surfaces::media::*;
use crate::agent_surfaces::store::{record_with, SurfaceInput};
use crate::agent_surfaces::validate::{is_link_url, is_media_url};
use crate::agent_surfaces::*;
use crate::comments::{self, Participant};
use crate::fs;
use crate::logging::LogBuffer;
use crate::tools::tests::mounted;

const DIGEST: &str = "aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00aa00";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent { agent_id: "a".into(), handle: "arch".into(), model: None, title: None }
}

// AUI-FR-GUUJ: the media sources and the link addresses that a surface takes.
#[test]
fn media_and_link_addresses_are_checked() {
    for good in
        ["https://example.com/a.png", "project:docs/img/a.png", &format!("attachment:{DIGEST}")]
    {
        assert!(is_media_url(good), "{good}");
    }
    for bad in [
        "http://example.com/a.png",
        "file:///etc/passwd",
        "data:image/png;base64,AAAA",
        "project:/etc/passwd",
        "project:../secret.png",
        "project:docs/../../x.png",
        "project:",
        "attachment:xyz",
        "javascript:alert(1)",
        "https://",
    ] {
        assert!(!is_media_url(bad), "{bad}");
    }
    for good in ["https://a2ui.org", "http://localhost:3000", "mailto:team@example.com"] {
        assert!(is_link_url(good), "{good}");
    }
    for bad in ["javascript:alert(1)", "file:///x", "tel:123", "https:"] {
        assert!(!is_link_url(bad), "{bad}");
    }
    let image = |url: &str| vec![json!({"id":"root","component":"Image","url":url})];
    validate_surface(&image("project:a.png"), &json!({})).expect("a project image");
    assert_eq!(validate_surface(&image("file:///a.png"), &json!({})).unwrap_err().rule, "bad_url");
    let open = |url: &str| -> Vec<Value> {
        vec![
            json!({"id":"root","component":"Button","child":"l","action":{"functionCall":{"@call":"openUrl","args":{"url":url}}}}),
            json!({"id":"l","component":"Text","text":"Open"}),
        ]
    };
    validate_surface(&open("https://a2ui.org"), &json!({})).expect("an https link");
    assert_eq!(
        validate_surface(&open("javascript:alert(1)"), &json!({})).unwrap_err().rule,
        "bad_url"
    );
}

struct Fixture {
    open: crate::tools::tests::Fixture,
    path: std::path::PathBuf,
    draft_id: String,
    thread_id: String,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = fs::RootFs::for_root(&path);
        let created = crate::drafts::create_draft_at_root(&root, Some("relay")).expect("draft");
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &comments::DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
            None,
            "@arch show me".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture { open: mounted(dir), path, draft_id: created.draft.id, thread_id: thread.id }
    }

    fn root(&self) -> fs::RootFs {
        fs::RootFs::for_root(&self.path)
    }

    fn record(&self, components: Vec<Value>) -> AgentSurface {
        static BUFFER: LogBuffer = LogBuffer::new();
        let origin =
            ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false);
        record_with(
            &self.open.handle(),
            &BUFFER,
            &self.root(),
            SurfaceInput {
                components: &components,
                data_model: &json!({}),
                message: "Look. <spoken>Look.</spoken>",
                agent: &agent(),
                origin: &origin,
            },
        )
        .expect("recorded")
    }
}

fn media(url: &str) -> Vec<Value> {
    vec![
        json!({"id":"root","component":"Column","children":["m"]}),
        json!({"id":"m","component":"Image","url":url}),
    ]
}

// AUI-FR-JUDL: a project file the surface names is read with its media type.
#[test]
fn a_named_project_image_is_read() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.path.join("docs")).unwrap();
    std::fs::write(f.path.join("docs/a.png"), [137u8, 80, 78, 71]).unwrap();
    let surface = f.record(media("project:docs/a.png"));
    let read =
        read_media_impl(&f.root(), &f.root(), &surface.id, "project:docs/a.png").expect("read");
    assert_eq!(read.media_type, "image/png");
    assert_eq!(
        base64::engine::general_purpose::STANDARD.decode(read.data).unwrap(),
        [137u8, 80, 78, 71]
    );
    assert_eq!(media_type_of("clip.WEBM"), Some("video/webm"));
    assert_eq!(media_type_of("song.mp3"), Some("audio/mpeg"));
    assert_eq!(media_type_of("notes.md"), None);
}

// AUI-FR-YXMD: a source the surface does not name, an `https` address, a
// file that is not media, a missing file, and a large file are refused.
#[test]
fn a_source_the_surface_does_not_name_is_refused() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.path.join("docs")).unwrap();
    std::fs::write(f.path.join("docs/a.png"), [1u8]).unwrap();
    std::fs::write(f.path.join("docs/notes.md"), "x").unwrap();
    let surface = f.record(vec![
        json!({"id":"root","component":"Column","children":["a","b","c","d","e"]}),
        json!({"id":"a","component":"Image","url":"project:docs/a.png"}),
        json!({"id":"b","component":"Image","url":"project:docs/notes.md"}),
        json!({"id":"c","component":"Image","url":"project:docs/missing.png"}),
        json!({"id":"d","component":"Video","url":"project:docs/big.mp4"}),
        json!({"id":"e","component":"Image","url":"https://example.com/a.png"}),
    ]);
    let file = std::fs::File::create(f.path.join("docs/big.mp4")).unwrap();
    file.set_len(MAX_MEDIA_BYTES + 1).unwrap();
    let read =
        |source: &str| read_media_impl(&f.root(), &f.root(), &surface.id, source).unwrap_err();
    assert_eq!(read("project:docs/other.png"), ERR_MEDIA_NOT_NAMED);
    assert_eq!(read("https://example.com/a.png"), ERR_MEDIA_NOT_NAMED);
    assert_eq!(read("project:docs/notes.md"), ERR_NOT_MEDIA);
    assert_eq!(read("project:docs/missing.png"), ERR_MEDIA_NOT_FOUND);
    assert_eq!(read("project:docs/big.mp4"), ERR_MEDIA_TOO_LARGE);
    assert_eq!(
        read_media_impl(&f.root(), &f.root(), "nope", "project:docs/a.png").unwrap_err(),
        ERR_SURFACE_NOT_FOUND
    );
}

// AUI-FR-YXMD: an attachment digest that no comment of the discussion holds
// is refused, and one that it holds is read.
#[test]
fn an_attachment_is_read_from_the_discussion_alone() {
    let f = Fixture::new();
    let root = f.root();
    let thread = comments::fold_discussion(&root, &f.draft_id)
        .into_iter()
        .find(|t| t.id == f.thread_id)
        .unwrap();
    let png = base64::engine::general_purpose::STANDARD.encode([137u8, 80, 78, 71, 1, 2]);
    let appended = comments::add_comment_to(
        &root,
        thread.log_ref(),
        &thread.id,
        "Here is the screen.".into(),
        Vec::new(),
        vec![comments::AttachmentInput::Inline {
            media_type: "image/png".into(),
            filename: "s.png".into(),
            data: png,
        }],
        comments::Stamp { by: &human(), at: "2999-01-01T00:00:00Z" },
    )
    .expect("appended");
    let digest = appended
        .comments
        .iter()
        .flat_map(|c| &c.attachments)
        .find_map(|a| match a {
            comments::Attachment::Blob { digest, .. } => Some(digest.clone()),
            _ => None,
        })
        .expect("a blob");
    let surface = f.record(media(&format!("attachment:{digest}")));
    let read =
        read_media_impl(&root, &root, &surface.id, &format!("attachment:{digest}")).expect("read");
    assert_eq!(read.media_type, "image/png");
    let unknown = f.record(media(&format!("attachment:{DIGEST}")));
    assert_eq!(
        read_media_impl(&root, &root, &unknown.id, &format!("attachment:{DIGEST}")).unwrap_err(),
        ERR_MEDIA_NOT_FOUND
    );
}

// AUI-FR-YXMD: a link that leads out of the project and a folder named like
// an image are refused, and nothing is read.
#[test]
fn a_link_out_of_the_project_and_a_folder_are_refused() {
    let f = Fixture::new();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("secret.png"), [1u8]).unwrap();
    std::fs::create_dir_all(f.path.join("docs/shot.png")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret.png"), f.path.join("docs/link.png"))
        .unwrap();
    let surface = f.record(vec![
        json!({"id":"root","component":"Column","children":["a","b"]}),
        json!({"id":"a","component":"Image","url":"project:docs/link.png"}),
        json!({"id":"b","component":"Image","url":"project:docs/shot.png"}),
    ]);
    let read = |source: &str| read_media_impl(&f.root(), &f.root(), &surface.id, source);
    #[cfg(unix)]
    assert!(read("project:docs/link.png").is_err(), "a link is never followed");
    assert_eq!(read("project:docs/shot.png").unwrap_err(), ERR_NOT_MEDIA);
}

// AUI-FR-YXMD: a source named by a binding, or by a component that is not
// media, is not named.
#[test]
fn a_source_named_by_a_binding_or_a_text_is_not_named() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.path.join("docs")).unwrap();
    std::fs::write(f.path.join("docs/a.png"), [1u8]).unwrap();
    let surface = f.record(vec![
        json!({"id":"root","component":"Column","children":["a","t"]}),
        json!({"id":"a","component":"Image","url":{"@path":"/src"}}),
        json!({"id":"t","component":"Text","text":"project:docs/a.png"}),
    ]);
    assert_eq!(
        read_media_impl(&f.root(), &f.root(), &surface.id, "project:docs/a.png").unwrap_err(),
        ERR_MEDIA_NOT_NAMED
    );
}

// AUI-FR-GUUJ: Video and AudioPlayer take the same sources as Image, and a
// refusal names the component and the property.
#[test]
fn every_media_component_checks_its_url() {
    for kind in ["Image", "Video", "AudioPlayer"] {
        let bad = vec![json!({"id":"root","component":kind,"url":"ftp://example.com/a"})];
        let violation = validate_surface(&bad, &json!({})).unwrap_err();
        assert_eq!(
            (violation.rule, violation.component_id.as_deref(), violation.property.as_deref()),
            ("bad_url", Some("root"), Some("url")),
            "{kind}"
        );
        let good = vec![json!({"id":"root","component":kind,"url":"https://example.com/a"})];
        validate_surface(&good, &json!({})).unwrap_or_else(|v| panic!("{kind}: {v:?}"));
    }
}
