//! The fixture of the plan tools (`PIR-plan-issue-read-tools.md`,
//! `PPT-propose-plan-changes-tool.md`): a mock application with a project
//! open, a fake GitHub tracker that holds a small plan, and one Map thread. No
//! test that uses it reaches a network.

use tauri::Manager;

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::comments::{Opening, Participant, Stamp};
use crate::delivery::github::tests::fake::{Harness, Mock};

/// The plan of `Harness::github_plan`: an epic (#1), a feature under it (#2),
/// and a task under the feature (#3) in "Milestone 1". The Types are Epic,
/// Feature, Task, and Bug. The Status options are Todo, Ready, In Progress, and
/// Done. The labels are `bug`, `docs`, and `Needs Review`.
pub(crate) struct PlanFixture {
    pub h: Harness,
    pub discussion_id: String,
}

pub(crate) fn human() -> Participant {
    Participant::Human { login: "author".into(), display_name: None, email: None }
}

pub(crate) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "planner".into(),
        model: None,
        title: Some(String::new()),
    }
}

impl PlanFixture {
    pub fn new() -> Self {
        let h = Harness::github_plan();
        // TLC-FR-13: a tool answers `no open project` from the mount of the
        // search index, so the fixture mounts the project there too.
        h.app.manage(crate::bm25_index::Bm25Indexer::default());
        // MPR-FR-IANE: an accept can ask for a follow-up turn, and no fixture
        // test runs a model.
        h.app.manage(crate::map_plan::DispatchSeam(std::sync::Arc::new(|_| Ok(()))));
        h.app.state::<crate::bm25_index::Bm25Indexer>().mount(&h.root());
        // GPP-FR-NLPG: the configuration is `valid` once a read of the Project
        // found it so. The load stores the read cache of the plan too.
        crate::delivery::github::load_plan_impl(&h.handle(), true).expect("a plan");
        let store = h.handle().state::<crate::project::ProjectState>().require_store().unwrap();
        let write = crate::comments::open_map_discussion_in(
            &store,
            None,
            Opening {
                body: "Plan the next step".into(),
                attachments: Vec::new(),
                spoken_original: None,
            },
            Stamp { by: &human(), at: "2026-03-01T10:00:00Z" },
        )
        .unwrap();
        PlanFixture { h, discussion_id: write.discussion.id }
    }

    /// Read the Project again, so the read cache holds what the fake holds now.
    pub fn refresh(&self) {
        crate::delivery::github::load_plan_impl(&self.app(), true).expect("a plan");
    }

    /// Close the project, so each tool refuses with `no open project`.
    pub fn close(&self) {
        self.h.close();
        self.app().state::<crate::bm25_index::Bm25Indexer>().clear();
    }

    pub fn app(&self) -> tauri::AppHandle<Mock> {
        self.h.handle()
    }

    /// The comments store of the open project.
    pub fn store(&self) -> crate::fs::RootFs {
        self.app().state::<crate::project::ProjectState>().require_store().unwrap()
    }

    pub fn roots(&self) -> OwnedRoots {
        OwnedRoots { worktree: self.h.root(), store: self.store() }
    }

    /// The origin of a turn of the Map thread.
    pub fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_map(self.discussion_id.clone())
    }

    /// MPR-FR-TQVZ: a passage thread of the file `spec.md`, which is not a Map
    /// thread, and the origin of a turn in it.
    pub fn passage_thread(&self) -> ConversationOrigin {
        let position = crate::comments::FragmentTarget::in_artifact("spec.md", 0, 4, "Spec");
        let thread = crate::comments::open_artifact_fragment_in(
            &self.store(),
            "spec.md",
            position,
            "File an issue for this".into(),
            Vec::new(),
            &human(),
            "2026-03-01T10:00:00Z",
        )
        .unwrap();
        ConversationOrigin::of(&thread)
    }

    /// The comments of the discussion of `origin`, in its own log.
    pub fn comments_of(&self, origin: &ConversationOrigin) -> Vec<crate::comments::Comment> {
        crate::comments::locate_discussion_of(&self.store(), &origin.target, origin.discussion_id())
            .map(|d| d.comments)
            .unwrap_or_default()
    }

    /// The comments of the Map thread.
    pub fn comments(&self) -> usize {
        crate::comments::fold_map_discussions(&self.store())
            .into_iter()
            .find(|d| d.id == self.discussion_id)
            .map(|d| d.comments.len())
            .unwrap_or(0)
    }
}
