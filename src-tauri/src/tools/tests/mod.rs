//! Tests for `TLC-tool-conventions.md`.
//!
//! These are the claims that bind *every* tool in the group, so most of them
//! run over both tools rather than picking one: a convention only one tool
//! honours is not a convention. Per-tool behaviour lives in
//! `skill_search/tests.rs` and `skill_list/tests.rs`.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;
use tauri::Manager;

pub(crate) mod document_fixture;

use super::*;
use crate::logging::{LogBuffer, LogFilter, LogLevel};
use crate::tools::spec_search::SpecSearchTool;
use crate::tools::{
    document_get, document_search, draft_read, draft_search, escalate_to_user, file_read, note_search, read_graduation_file,
    skill_list, skill_load, skill_search, spec_search,
};

/// Install an agent session over `root` and return its id (FSA-FR-29).
///
/// `read_file` names a session rather than holding an instance, so a fixture
/// that wants a working `read_file` has to open one.
pub(crate) fn agent_session(
    app: &tauri::App<tauri::test::MockRuntime>,
    root: &std::path::Path,
    id: &str,
) -> String {
    let state = app.state::<crate::fs::FsAccessState>();
    state
        .install_for_worktree(root)
        .expect("a test root is a real directory");
    state.open_agent_session(id).expect("a project is open");
    id.to_string()
}

/// The definition of every tool in this group, resolved through `rig` exactly
/// as a provider would receive it.
fn definitions() -> Vec<rig::completion::ToolDefinition> {
    let app = closed_project();
    let handle = app.handle().clone();
    let handle2 = handle.clone();
    let handle3 = handle.clone();
    vec![
        rig::tool::portable_tool_definition(&skill_search::SkillSearchTool::new(handle.clone())),
        rig::tool::portable_tool_definition(&skill_list::SkillListTool::new(handle.clone())),
        rig::tool::portable_tool_definition(&skill_load::SkillLoadTool::new(handle.clone())),
        rig::tool::portable_tool_definition(&spec_search::SpecSearchTool::new(handle.clone())),
        rig::tool::portable_tool_definition(&file_read::FileReadTool::new(
            handle.clone(),
            "definitions",
        )),
        rig::tool::portable_tool_definition(&draft_search::DraftSearchTool::new(
            handle.clone(),
            "definitions",
        )),
        rig::tool::portable_tool_definition(&draft_read::DraftReadTool::new(
            handle.clone(),
            "definitions",
        )),
        rig::tool::portable_tool_definition(&note_search::NoteSearchTool::new(
            handle.clone(),
            "definitions",
        )),
        rig::tool::portable_tool_definition(&document_search::SearchDocumentsTool::new(
            handle.clone(),
        )),
        rig::tool::portable_tool_definition(&document_get::GetDocumentTool::new(handle.clone())),
        // RGF-FR-02 / ESU-FR-19: both are constructed against one graduation
        // run, so this one needs a run id and an execution directory to exist
        // at all. Neither reaches the schema — two instances built for two runs
        // return byte-identical definitions.
        rig::tool::portable_tool_definition(
            &read_graduation_file::ReadGraduationFileTool::new(
                handle.clone(),
                "run-definitions",
                std::env::temp_dir(),
            )
            .expect("a temp directory is a real directory"),
        ),
        // AUC-FR-02: constructed per turn, so this one needs a conversation to
        // be bound to. Its *definition* is fixed application data all the same,
        // which is what every test over this list checks.
        rig::tool::portable_tool_definition(&ask_user_comment::AskUserCommentTool::new(
            handle,
            crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(&std::env::temp_dir()), store: crate::fs::RootFs::for_root(&std::env::temp_dir()) },
            crate::agent_conversations::ConversationOrigin::stub_artifact("definitions", "a.md", true),
            crate::comments::Participant::Agent {
                agent_id: "definitions".into(),
                handle: "definitions".into(),
                model: None,
                title: None,
            },
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )),
        // PDC-FR-02: constructed per turn like the one above, and its definition
        // fixed on exactly the same terms. Bound to a draft origin because that
        // is the only kind it is ever attached to (CVL-FR-08) — the binding does
        // not reach the definition, which is what this list is about.
        rig::tool::portable_tool_definition(&propose_draft_changes::ProposeDraftChangesTool::new(
            handle2,
            crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(&std::env::temp_dir()), store: crate::fs::RootFs::for_root(&std::env::temp_dir()) },
            crate::agent_conversations::ConversationOrigin::stub_draft("definitions", "definitions", false),
            crate::comments::Participant::Agent {
                agent_id: "definitions".into(),
                handle: "definitions".into(),
                model: None,
                title: None,
            },
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )),
        // ADQ-FR-KZNV: constructed per turn like the two above, and held to
        // every convention this list checks. Bound to a discussion origin
        // because that is the only kind it is ever attached to (CVL-FR-08,
        // ADQ-FR-LFDX) — the binding does not reach the definition.
        rig::tool::portable_tool_definition(
            &ask_discussion_questions::AskDiscussionQuestionsTool::new(
                handle3,
                crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(&std::env::temp_dir()), store: crate::fs::RootFs::for_root(&std::env::temp_dir()) },
                crate::agent_conversations::ConversationOrigin::stub_draft("definitions", "definitions", false),
                crate::comments::Participant::Agent {
                    agent_id: "definitions".into(),
                    handle: "definitions".into(),
                    model: None,
                    title: None,
                },
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            ),
        ),
    ]
}

/// Every source file of the group, for the tests that read the group itself
/// rather than call it. Listed once so a tool added tomorrow is swept by all of
/// them or by none, rather than by whichever list someone remembered.
const GROUP_SOURCES: [(&str, &str); 16] = [
    ("tools.rs", include_str!("../../tools.rs")),
    ("skill_search.rs", include_str!("../skill_search.rs")),
    ("skill_list.rs", include_str!("../skill_list.rs")),
    ("skill_load.rs", include_str!("../skill_load.rs")),
    ("spec_search.rs", include_str!("../spec_search.rs")),
    ("file_read.rs", include_str!("../file_read.rs")),
    ("draft_search.rs", include_str!("../draft_search.rs")),
    ("draft_read.rs", include_str!("../draft_read.rs")),
    ("note_search.rs", include_str!("../note_search.rs")),
    ("document_search.rs", include_str!("../document_search.rs")),
    ("document_get.rs", include_str!("../document_get.rs")),
    ("ask_user_comment.rs", include_str!("../ask_user_comment.rs")),
    // ADQ-FR-JAQE, ADQ-FR-UIYD: swept with the rest of the group, so the claims
    // that it reaches no network, holds no credential, and publishes no
    // registry are checked rather than asserted.
    (
        "ask_discussion_questions.rs",
        include_str!("../ask_discussion_questions.rs"),
    ),
    (
        "propose_draft_changes.rs",
        include_str!("../propose_draft_changes.rs"),
    ),
    ("web_search.rs", include_str!("../web_search.rs")),
    ("web_fetch.rs", include_str!("../web_fetch.rs")),
];

/// The provider-native members of the group, which are entries rather than
/// implementations (TLC-FR-21). Swept together because the two claims that bind
/// them are claims about *both*: a search and a fetch are two tools the model
/// sees separately, and neither is ever offered without the other.
const NATIVE_SOURCES: [(&str, &str); 2] = [
    ("web_search.rs", include_str!("../web_search.rs")),
    ("web_fetch.rs", include_str!("../web_fetch.rs")),
];

/// Poll a tool's future exactly once.
///
/// TLC-FR-16 says a tool resolves without blocking on background work, so one
/// poll is not merely enough — a `Pending` here is a failure of that
/// requirement, which is what this asserts.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("a tool must not block (TLC-FR-16)"),
    }
}


// ---------------------------------------------------------------------------
// Fixtures shared by every suite in this group
// ---------------------------------------------------------------------------
//
// The tools delegate wholesale to `DSL-dynamic-skills-loading.md`, so what is
// worth pinning is their behaviour against a *real* registry and a *real* BM25
// index — a stub would answer the ranking claims (SST-FR-04, SST-FR-11) by
// construction and prove nothing. Every fixture here therefore builds a temp
// project, mounts it, and runs one real pass.
//
// They live in this file rather than beside it because `fs/access_tests.rs`
// sweeps every backend source for direct `std::fs` use and exempts `tests.rs`
// alone, fixtures being exactly what it expects to find building trees there.

use crate::bm25_index::{Bm25Indexer, PassScope};
use crate::progress::ProgressRegistry;
use crate::scanning::CandidateStore;

/// A mock app managing the state a pass touches — the same set
/// `bm25_index/tests.rs` mounts, since a pass here is a pass like any other.
pub(crate) fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    let app = tauri::test::mock_app();
    app.manage(Bm25Indexer::default());
    app.manage(ProgressRegistry::default());
    app.manage(CandidateStore::default());
    app.manage(crate::fs::FsAccessState::default());
    app
}

/// Write `<root>/<folder>/<name>/SKILL.md` with `front` as its frontmatter and
/// `body` beneath it.
pub(crate) fn write_skill(root: &std::path::Path, folder: &str, name: &str, front: &str, body: &str) {
    let dir = root.join(folder).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\n{front}\n---\n\n{body}\n"),
    )
    .unwrap();
}

/// A mounted, fully indexed project.
///
/// The `TempDir` is returned alongside so the caller keeps the tree alive for
/// the duration of the test — dropping it would delete the project out from
/// under the indexes.
pub(crate) struct Fixture {
    pub(crate) app: tauri::App<tauri::test::MockRuntime>,
    pub(crate) _dir: TempDir,
}

impl Fixture {
    pub(crate) fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    /// Re-run a pass, for the tests that change a skill on disk and assert the
    /// tools follow it (SLT-FR-16).
    pub(crate) fn reindex(&self) {
        let handle = self.handle();
        // The scan behind a pass is cached, and the watcher is what drops it on
        // a structural change (ASC-FR-10). Nothing here runs a watcher, so a
        // test that creates or deletes a file has to stand in for one — without
        // this, a pass re-reads the paths it already knew and a newly added file
        // is invisible however many times it runs.
        self.app.state::<crate::scanning::CandidateStore>().invalidate();
        let indexer = self.app.state::<Bm25Indexer>();
        let root = indexer.root().expect("mounted");
        let generation = indexer.generation();
        indexer.run_pass(
            &handle,
            &crate::fs::RootFs::for_root(&root),
            PassScope::ALL,
            generation,
        );
    }
}

/// Mount `dir` and build every index against it.
pub(crate) fn mounted(dir: TempDir) -> Fixture {
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = mock_app();
    let handle = app.handle().clone();
    {
        let indexer = app.state::<Bm25Indexer>();
        indexer.mount(&crate::fs::RootFs::for_root(&root));
        let generation = indexer.generation();
        indexer
            .run_pass(
                &handle,
                &crate::fs::RootFs::for_root(&root),
                PassScope::ALL,
                generation,
            )
            .expect("the pass publishes");
    }
    Fixture { app, _dir: dir }
}

/// A project holding no skill at all, mounted and indexed.
pub(crate) fn empty_project() -> Fixture {
    mounted(TempDir::new().unwrap())
}

/// An app with nothing mounted — the "no project open" state every tool refuses
/// against (TLC-FR-13).
pub(crate) fn closed_project() -> tauri::App<tauri::test::MockRuntime> {
    mock_app()
}

/// The project most tests run against: three eligible skills across three
/// ecosystems, each describing a distinct task, plus one whose *body* carries a
/// term its description does not.
pub(crate) fn demo_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".claude/skills",
        "analyst",
        "name: analyst\ndescription: Review a specification for internal contradictions and author new requirement documents.",
        "# Analyst\n\nThis body mentions sarcophagus, which the description does not.",
    );
    write_skill(
        root,
        ".codex/skills",
        "deployer",
        "name: deployer\ndescription: Deploy a built container image to the production cluster and roll it back.",
        "# Deployer\n\nSteps for deployment.",
    );
    write_skill(
        root,
        ".github/skills",
        "formatter",
        "name: formatter\ndescription: Reformat source files to the repository code style.",
        "# Formatter\n\nHow to reformat.",
    );
    mounted(dir)
}

/// A project whose descriptions deliberately **overlap**, so a ranking query
/// returns several matches rather than one.
///
/// `demo_project`'s three descriptions are disjoint by design, which makes it
/// the wrong fixture for an ordering assertion: every query against it returns
/// a single match, and a one-element vector is sorted under every ordering
/// there is. Everything here shares the word "review", so a query has a real
/// set to put in order and a reversed sort has somewhere to show.
pub(crate) fn ranking_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".claude/skills",
        "spec-reviewer",
        "name: spec-reviewer\ndescription: Review a specification for internal contradictions, ambiguity, and missing requirements.",
        "# Spec reviewer",
    );
    write_skill(
        root,
        ".claude/skills",
        "diff-reviewer",
        "name: diff-reviewer\ndescription: Review a pull request diff for defects and style violations.",
        "# Diff reviewer",
    );
    write_skill(
        root,
        ".claude/skills",
        "migration-reviewer",
        "name: migration-reviewer\ndescription: Review a database migration for destructive operations.",
        "# Migration reviewer",
    );
    mounted(dir)
}

/// A project where one name is held by three ecosystems and another by one.
///
/// The `review` skills carry *different* descriptions on purpose: identical text
/// would make the search's "keep the better match" rule (SST-FR-12) untestable,
/// because either survivor would satisfy an assertion about score. The `.codex`
/// one is the one that matches "pull request diff", so the tool that keeps the
/// higher score and the tool that keeps the first path disagree about which
/// file stands behind the name — which is exactly what SLT-FR-14 now permits and
/// what LSK-FR-10 refuses to guess between.
pub(crate) fn collision_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_skill(
        root,
        ".claude/skills",
        "review",
        "name: review\ndescription: Review a specification document for internal contradictions.",
        "# Claude review\n\nThe claude copy's instructions.",
    );
    write_skill(
        root,
        ".codex/skills",
        "review",
        "name: review\ndescription: Review a pull request diff for defects, regressions and style violations.",
        "# Codex review\n\nThe codex copy's instructions.",
    );
    write_skill(
        root,
        ".opencode/skills",
        "review",
        "name: review\ndescription: Review a database migration for destructive operations.",
        "# Opencode review\n\nThe opencode copy's instructions.",
    );
    write_skill(
        root,
        ".claude/skills",
        "analyst",
        "name: analyst\ndescription: Author new requirement documents from a stated intent.",
        "# Analyst\n\nFirst line of the analyst body.\n\nSecond paragraph.",
    );
    // A second uncolliding skill, and one that sorts *after* the deduplicated
    // entry: with survivors on both sides of it, an implementation that
    // appended the kept entries rather than leaving them in place is
    // distinguishable from one that preserved the order (SLT-FR-08).
    write_skill(
        root,
        ".codex/skills",
        "packager",
        "name: packager\ndescription: Build and sign a release package for distribution.",
        "# Packager\n\nHow to package.",
    );
    mounted(dir)
}

/// Two skills that tie on score, in an order the folder walk would not produce.
///
/// The four ecosystem folders are walked claude → codex → github → opencode,
/// which is already ascending by path prefix, so a fixture spread across them
/// cannot tell the explicit sort from the walk. These two sit in one folder
/// with names whose sorted order is the opposite of neither — `alpha` before
/// `zulu` — and carry identical descriptions so their scores are equal.
pub(crate) fn tied_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let front = "description: Review a pull request diff for defects and style violations.";
    write_skill(dir.path(), ".claude/skills", "zulu", front, "# Z");
    write_skill(dir.path(), ".claude/skills", "alpha", front, "# A");
    mounted(dir)
}

/// A record's field names, sorted.
pub(crate) fn sorted_keys(record: &crate::logging::LogRecord) -> Vec<&str> {
    let mut keys: Vec<&str> = record.fields.keys().map(String::as_str).collect();
    keys.sort();
    keys
}

/// Every path under `root` with its contents, for the read-only assertion.
pub(crate) fn tree_snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let relative = path.strip_prefix(base).unwrap_or(&path);
                out.push((relative.to_string_lossy().to_string(), bytes));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

mod conventions;
mod native;
mod refusals;
mod surface;
