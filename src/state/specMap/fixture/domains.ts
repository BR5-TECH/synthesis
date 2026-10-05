/**
 * SMP-FR-HWIC: the demo index the stub `"load specification map"` serves.
 *
 * Every spec node names a real spec file under `specifications/`. The
 * hierarchy, the summaries, and the completeness states are illustrative: they
 * stand in for the indexing backend until it exists.
 */
import type { IndexNode, SpecNode } from "../types";

const STATE = { v: "verified", i: "built", d: "drafted", g: "gap" } as const;

function S(
  code: string,
  path: string,
  label: string,
  requirements: number,
  state: keyof typeof STATE,
  summary: string,
): SpecNode {
  return {
    code,
    path,
    label,
    summary,
    requirements,
    scenarios: Math.round(requirements * 0.74),
    state: STATE[state],
  };
}

function N(id: string, label: string, summary: string, children: IndexNode[] | SpecNode[], hue?: string): IndexNode {
  return hue ? { id, label, summary, hue, children } : { id, label, summary, children };
}

const authoring = N("authoring", "authoring", "Everything the architect writes and reviews by hand. Drafts, comments and the editor. Largest surface area in the corpus.", [
  N("drafts", "drafts & proposals", "Drafts hold work in progress until graduation. Agents propose changes; the author accepts or rejects them.", [
    N("drafts-store", "storage", "Where a draft, its history, assets and statistics live on disk.", [
      S("DRS", "core/DRS-draft-storage.md", "draft storage", 73, "i", "Draft files, metadata and folders on disk."),
      S("DHS", "core/DHS-draft-history.md", "draft history", 28, "i", "Every revision of a draft, kept and restorable."),
      S("DAS", "core/DAS-draft-assets.md", "draft assets", 28, "d", "Images and attachments referenced from a draft."),
      S("DSS", "core/DSS-draft-statistics-storage.md", "draft statistics", 37, "d", "Counters a draft accumulates over its life."),
    ]),
    N("drafts-prop", "change proposals", "Agent-authored edits presented for review rather than written directly.", [
      S("DCP", "core/DCP-draft-change-proposals.md", "draft change proposals", 38, "i", "Proposed draft edits, held until accepted or rejected."),
      S("PCP", "core/PCP-prompt-change-proposals.md", "prompt change proposals", 29, "d", "Proposed prompt edits with the same review path."),
      S("PDC", "tools/PDC-propose-draft-changes-tool.md", "propose draft changes", 22, "i", "Tool an agent calls to propose draft edits."),
      S("PPC", "tools/PPC-propose-prompt-changes-tool.md", "propose prompt changes", 21, "d", "Tool an agent calls to propose prompt edits."),
      S("PCR", "ui/PCR-prompt-change-review.md", "prompt change review", 34, "d", "Reviewing a proposed prompt edit in place."),
    ]),
    N("drafts-ui", "authoring surfaces", "Panels and tabs where drafts are created, discussed and graduated.", [
      S("DRP", "ui/DRP-drafts-panel.md", "drafts panel", 37, "i", "Vertical panel listing every draft in the project."),
      S("DCR", "ui/DCR-draft-change-review.md", "draft change review", 39, "d", "Accept or reject each proposed change in place."),
      S("DFI", "ui/DFI-draft-information.md", "draft information", 18, "g", "Metadata header shown above an open draft."),
      S("NAW", "ui/NAW-new-artifact.md", "new artifact", 73, "d", "The tab where a draft becomes a real artifact."),
      S("NTA", "ui/NTA-new-typed-artifact.md", "new typed artifact", 16, "i", "Modal for creating one typed artifact directly."),
      S("DDS", "ui/DDS-draft-discussion.md", "draft discussion", 32, "i", "The conversation column beside a draft."),
      S("DQA", "ui/DQA-discussion-question-answering.md", "question answering", 46, "d", "Answering the questions an agent asks in a discussion."),
    ]),
  ]),
  N("comments", "comments", "Threaded remarks anchored to document ranges. Agents answer in the same thread the author asked in.", [
    N("cm-store", "storage", "How comment threads and anchors survive edits to the text.", [
      S("CMS", "core/CMS-comments-storage.md", "comments storage", 82, "i", "Threads, anchors and reanchoring after the file changes."),
    ]),
    N("cm-ui", "surfaces", "Where comments appear: inline, in a panel, and as agent turns.", [
      S("CMT", "ui/CMT-comments.md", "comments", 61, "i", "Inline comment threads inside the editor."),
      S("CMP", "ui/CMP-comments-panel.md", "comments panel", 30, "i", "Panel listing every thread in the open project."),
      S("CTA", "ui/CTA-comment-agent-turns.md", "comment agent turns", 117, "d", "Agent replies rendered as turns inside a thread."),
    ]),
    N("cm-tools", "tools", "The tools an agent uses to ask the author a question.", [
      S("AUC", "tools/AUC-ask-user-comment-tool.md", "ask user comment", 19, "i", "Agent posts a question as a comment."),
      S("ADQ", "tools/ADQ-ask-discussion-questions-tool.md", "ask discussion questions", 32, "d", "Agent asks structured questions in a discussion."),
    ]),
  ]),
  N("editor", "editor & documents", "The document surface itself: WYSIWYG, raw text, diffs and tabs. Every other surface opens into it.", [
    N("ed-core", "editing", "Writing Markdown: rich mode, raw mode, find, external changes.", [
      S("EDT", "ui/EDT-editor.md", "editor", 48, "v", "Markdown editing, saving, dirty state and retained buffers."),
      S("EFR", "ui/EFR-editor-find-replace.md", "find & replace", 46, "v", "Find and replace within the open document."),
      S("ESH", "ui/ESH-editor-source-files.md", "source files", 41, "i", "Editing non-Markdown source files as plain text."),
      S("EXC", "ui/EXC-editor-external-change.md", "external change", 25, "i", "What happens when a file changes on disk."),
    ]),
    N("ed-diff", "diff, flow & tabs", "Comparing versions, editing flows on canvas, and the tab strip that holds them.", [
      S("DFV", "ui/DFV-diff-viewer.md", "diff viewer", 59, "i", "Side-by-side and rich diffs with an editable target."),
      S("FLO", "ui/FLO-flow.md", "flow canvas", 48, "d", "Node canvas for harnesses and coverage views."),
      S("TAB", "ui/TAB-tabs.md", "tabs", 43, "v", "Tab lifecycle, ordering, pinning and closures."),
      S("HVW", "ui/HVW-history-viewer.md", "history viewer", 10, "g", "Reading and restoring an earlier version."),
    ]),
  ]),
], "#6BD7AA");

const graduation = N("graduation", "graduation", "The autonomous path from specification to merged code. Longest running feature and the least verified.", [
  N("grad-core", "graduation loop", "The agent loop that turns a graduated draft into an implementation, run by run.", [
    N("grad-loop", "loop", "The graduation contract, the loop that drives it, and what it records.", [
      S("GRD", "core/GRD-graduation.md", "graduation", 34, "d", "The whole graduation contract, stage by stage."),
      S("GRL", "ai/GRL-graduation-loop.md", "graduation loop", 27, "d", "The agent loop driving a graduation to completion."),
      S("GLG", "ai/GLG-graduation-loop-logging.md", "loop logging", 9, "i", "What the loop records for later reading."),
    ]),
    N("grad-exec", "execution", "Starting a run, supervising it, rebasing it and publishing the result.", [
      S("GXD", "core/GXD-graduation-execution.md", "execution", 19, "d", "Starting and supervising an agent run."),
      S("GSU", "core/GSU-graduation-start.md", "start", 12, "i", "Preconditions checked before a graduation begins."),
      S("GRB", "core/GRB-graduation-rebase.md", "rebase", 35, "d", "Replaying a run's work onto a moved upstream."),
      S("GHP", "core/GHP-github-publication.md", "github publication", 39, "g", "Publishing a finished run to GitHub."),
    ]),
    N("grad-obs", "observability", "Run logs, progress and the tests that keep a long run honest.", [
      S("GOB", "core/GOB-graduation-observability.md", "observability", 14, "d", "Events every graduation stage emits."),
      S("GRS", "core/GRS-graduation-run-log-storage.md", "run log storage", 61, "i", "Where run logs are written and retained."),
      S("GTE", "infra/GTE-graduation-end-to-end-tests.md", "end-to-end tests", 18, "d", "Whole-run tests against the agent mock."),
    ]),
  ]),
  N("grad-ui", "graduation surfaces", "What the architect watches while a graduation runs, and reviews when it finishes.", [
    N("grad-watch", "live", "Runs list, log window, progress and escalations raised mid-run.", [
      S("GRU", "ui/GRU-graduation-runs.md", "graduation runs", 28, "d", "Live list of graduations with their stages."),
      S("GLW", "ui/GLW-graduation-log-window.md", "log window", 63, "d", "Detached window streaming one run log."),
      S("GEA", "ui/GEA-graduation-escalation-answering.md", "escalation answering", 47, "g", "Answering a question the agent escalated."),
      S("GRT", "ui/GRT-graduation-restart.md", "restart", 7, "g", "Restarting a graduation from a chosen stage."),
      S("RPV", "ui/RPV-run-progress.md", "run progress", 28, "i", "The stage track of a run in flight."),
    ]),
    N("grad-review", "history & start", "Starting a run and reading the runs that came before.", [
      S("GRH", "ui/GRH-graduation-history.md", "graduation history", 13, "d", "Every past graduation, searchable."),
      S("GSD", "ui/GSD-graduation-start-dialog.md", "start dialog", 13, "i", "The dialog that confirms a graduation start."),
      S("RUN", "ui/RUN-runs.md", "runs", 21, "i", "The bottom panel that lists agent runs."),
    ]),
  ]),
], "#C19EFF");

const agents = N("agents", "agents", "Agent conversations, vendor integrations and the tools agents may call. The engine room of the product.", [
  N("conv", "agents & conversations", "One conversation per task, presented as overlay, tab or bookmark. Registry decides which agent runs.", [
    N("ag-conv", "conversations", "The loop, its storage, and how a conversation is presented.", [
      S("CVL", "ai/CVL-conversation-loop.md", "conversation loop", 44, "i", "Turn-by-turn loop between author and agent."),
      S("AGC", "core/AGC-agent-conversations.md", "agent conversations", 51, "i", "Conversation records, turns and persistence."),
      S("CVP", "ui/CVP-conversation-presentation.md", "conversation presentation", 66, "d", "Overlay, tab and bookmark presentation of a conversation."),
    ]),
    N("ag-reg", "registry", "Which agents exist and what each is currently doing.", [
      S("AGR", "core/AGR-agent-registry.md", "agent registry", 24, "i", "Declared agents, their models and defaults."),
      S("AGV", "core/AGV-agent-activity.md", "agent activity", 16, "i", "What each agent is doing right now."),
      S("AGT", "ui/AGT-agents.md", "agents", 43, "i", "The roster where agents are declared and tuned."),
    ]),
    N("ag-int", "integrations", "Vendor APIs, CLI executors and the adapters between them.", [
      S("AIC", "core/AIC-agentic-integrations.md", "agentic integrations", 34, "i", "Running vendor CLIs inside pinned containers."),
      S("AAP", "core/AAP-ai-api-integrations.md", "ai api integrations", 36, "i", "Direct model API calls and their credentials."),
      S("AII", "ui/AII-ai-integrations.md", "ai integrations", 56, "d", "Settings surface for models, keys and adapters."),
      S("ADP", "core/ADP-adapters.md", "adapters", 12, "i", "Common shape every integration implements."),
      S("EAC", "tools/EAC-execute-agent-cli.md", "execute agent cli", 52, "i", "Executing an agentic CLI and streaming its output."),
    ]),
  ]),
  N("tools", "agent tools", "The bounded tool surface an agent may call. Search, read, web, and the conventions all tools share.", [
    N("tl-search", "search", "Finding specs, skills, drafts and notes by relevance.", [
      S("SPS", "tools/SPS-specification-search-tool.md", "specification search", 19, "v", "Search the specification corpus by query."),
      S("SST", "tools/SST-skill-search-tool.md", "skill search", 16, "v", "Search available skills by query."),
      S("SLT", "tools/SLT-skill-list-tool.md", "skill list", 18, "v", "List every skill the project exposes."),
      S("DST", "tools/DST-draft-search-tool.md", "draft search", 21, "i", "Search drafts by content and metadata."),
      S("NST", "tools/NST-note-search-tool.md", "note search", 20, "i", "Search notes attached to project entities."),
    ]),
    N("tl-read", "read", "Reading files, drafts and graduation artifacts under access rules.", [
      S("RFT", "tools/RFT-read-file-tool.md", "read file", 20, "v", "Read a project file within allowed roots."),
      S("RDT", "tools/RDT-read-draft-tool.md", "read draft", 18, "i", "Read a draft and its current revision."),
      S("RGF", "tools/RGF-read-graduation-file-tool.md", "read graduation file", 16, "i", "Read a file produced by a graduation."),
    ]),
    N("tl-web", "web", "Fetching and searching the open web from a run.", [
      S("WFT", "tools/WFT-web-fetch-tool.md", "web fetch", 16, "i", "Fetch one URL and return readable text."),
      S("WST", "tools/WST-web-search-tool.md", "web search", 14, "i", "Search the web and return ranked results."),
    ]),
    N("tl-conv", "conventions", "Rules every tool obeys, plus escalation and skill loading.", [
      S("TLC", "tools/TLC-tool-conventions.md", "tool conventions", 27, "v", "Naming, errors and limits shared by all tools."),
      S("ESU", "tools/ESU-escalate-to-user-tool.md", "escalate to user", 12, "i", "Pausing a run to ask the author."),
      S("LSK", "tools/LSK-load-skill-tool.md", "load skill", 19, "v", "Loading a skill into the running context."),
    ]),
  ]),
], "#E0A93B");

const workspace = N("workspace", "workspace", "The shell the architect works inside: zones, panels, windows, and the Git surface beneath them.", [
  N("shell", "shell & navigation", "Six zones, one navigation map, and the windows that open over them. The most settled feature.", [
    N("sh-zones", "zones", "Layout of the main window and the routes between surfaces.", [
      S("OVW", "ui/OVW-overview.md", "UI overview", 14, "v", "Surface inventory and the navigation map between them."),
      S("SNV", "ui/SNV-shell-navigation.md", "shell navigation", 71, "v", "Zone sizing, persistence and keyboard navigation."),
      S("STB", "ui/STB-status-bar.md", "status bar", 31, "v", "Service strip: settings, progress, conventions, diffstat."),
      S("ACT", "ui/ACT-action-control.md", "action control", 28, "i", "Shared control for starting and stopping work."),
    ]),
    N("sh-panels", "panels", "Vertical-panel surfaces and the dashboard the project opens onto.", [
      S("LIB", "ui/LIB-library.md", "project panel", 21, "v", "File tree of the active worktree."),
      S("LCM", "ui/LCM-library-context-menu.md", "context menu", 11, "i", "Actions offered on a tree row."),
      S("NTS", "ui/NTS-notes.md", "notes", 31, "i", "Notes attached to the entity in focus."),
      S("DSH", "ui/DSH-dashboard.md", "dashboard", 19, "i", "Widgets summarising the open project."),
      S("SCH", "ui/SCH-search.md", "search", 21, "i", "Universal search across artifacts and runs."),
    ]),
    N("sh-settings", "settings", "Two native settings windows, one shared preference store.", [
      S("SWN", "ui/SWN-settings-windows.md", "settings windows", 19, "v", "Child-window rules for both settings surfaces."),
      S("SET", "ui/SET-project-settings.md", "project settings", 27, "i", "Per-project configuration and bindings."),
      S("GLS", "ui/GLS-global-settings.md", "global settings", 35, "i", "Theme, fonts and application-wide preferences."),
    ]),
    N("sh-windows", "modal windows", "Small windows that create a file, folder or commit.", [
      S("NFI", "ui/NFI-new-file.md", "new file", 14, "v", "Creating a file and opening it."),
      S("NFW", "ui/NFW-new-folder.md", "new folder", 13, "v", "Creating a folder in the tree."),
      S("CMW", "ui/CMW-commit-message.md", "commit message", 18, "i", "Composing a commit message before committing."),
    ]),
    N("sh-notify", "notifications", "OS notifications and the log surface behind them.", [
      S("NTF", "ui/NTF-notifications.md", "notifications", 39, "d", "OS notifications and the surfaces they open."),
      S("LOG", "ui/LOG-logs.md", "logs", 22, "i", "Application log surface in the bottom panel."),
    ]),
    N("sh-map", "specification map", "This map: its canvas, nodes, edges, inspector and planning tools.", [
      S("SMP", "ui/SMP-specification-map.md", "specification map", 17, "d", "The Map tab, its toolbar and its session."),
      S("SMZ", "ui/SMZ-specification-map-zoom.md", "map zoom", 15, "d", "Semantic zoom, panning and the focused subtree."),
      S("SMN", "ui/SMN-specification-map-nodes.md", "map nodes", 16, "d", "Cards, boxes and chips at every level."),
      S("SME", "ui/SME-specification-map-edges.md", "map edges", 10, "d", "Dependency edges aggregated to the level shown."),
      S("SMI", "ui/SMI-specification-map-inspector.md", "map inspector", 15, "d", "The reading pane for the selected node."),
      S("SMO", "ui/SMO-specification-map-organization.md", "map organization", 14, "g", "Creating, moving and deleting map nodes."),
      S("SMD", "ui/SMD-specification-map-drafts.md", "map drafts", 11, "g", "Planning new work as drafts on the map."),
    ]),
  ]),
  N("git", "git & changes", "A project is a Git repository. Worktrees, changes, commits and GitHub credentials live here.", [
    N("git-engine", "engine", "Git operations and the change set they produce.", [
      S("GTC", "core/GTC-git.md", "git", 33, "i", "Repository operations the app performs."),
      S("CHC", "core/CHC-changes.md", "changes", 22, "v", "Uncommitted and branch comparisons with totals."),
    ]),
    N("git-ui", "surfaces", "The panels where changes are staged, committed and pushed.", [
      S("CHG", "ui/CHG-changes.md", "changes panel", 66, "i", "Reviewing, staging and committing changes."),
      S("GIT", "ui/GIT-git.md", "git panel", 11, "i", "Branches, remotes and push from one panel."),
    ]),
    N("git-auth", "authentication", "GitHub tokens, stored once and picked per push.", [
      S("GTS", "core/GTS-github-token-storage.md", "token storage", 17, "i", "Tokens kept in the application vault."),
      S("GHA", "ui/GHA-github-authentication.md", "github authentication", 22, "d", "Picking and binding a token to a repository."),
    ]),
    N("git-wt", "worktrees & streams", "One active worktree is the content root every surface reads.", [
      S("WTC", "core/WTC-worktree-context.md", "worktree context", 28, "i", "Which worktree the project currently reads."),
      S("WTS", "ui/WTS-worktree-selector.md", "worktree selector", 37, "d", "Switching the active worktree from top chrome."),
      S("WKS", "core/WKS-work-streams.md", "work streams", 59, "d", "Parallel streams of work on one repository."),
      S("WSS", "ui/WSS-work-stream-selector.md", "work stream selector", 38, "d", "Choosing the stream a session works in."),
    ]),
  ]),
], "#7A8BFF");

const platform = N("platform", "platform", "Storage, indexing, runtime services and the build. Invisible to users, load-bearing for everything above.", [
  N("storage", "storage & runtime", "Where settings, secrets and project bytes live, plus indexing and the small runtime services.", [
    N("pf-store", "storage", "Settings, secrets, project files and filesystem access rules.", [
      S("PSS", "core/PSS-project-settings-storage.md", "project settings storage", 39, "i", "Per-project configuration on disk."),
      S("GSS", "core/GSS-global-settings-storage.md", "global settings storage", 45, "i", "Application preferences, stored once per host."),
      S("ASV", "core/ASV-application-secret-vault.md", "secret vault", 33, "i", "Encrypted store for keys and tokens."),
      S("PST", "core/PST-project-storage.md", "project storage", 43, "i", "Reading and writing project files safely."),
      S("FSA", "core/FSA-filesystem-access.md", "filesystem access", 32, "v", "Which paths the app and agents may touch."),
      S("RMS", "core/RMS-repository-machine-storage.md", "repository machine storage", 17, "i", "Per-machine state kept beside a repository."),
      S("NTC", "core/NTC-notes-storage.md", "notes storage", 25, "i", "Where notes and their scopes are kept."),
    ]),
    N("pf-index", "indexing", "Scanning artifacts and ranking them for search. Feeds this map.", [
      S("ASC", "core/ASC-artifact-scanning.md", "artifact scanning", 24, "i", "Discovering artifacts and watching for changes."),
      S("BMI", "core/BMI-bm25-indexing.md", "bm25 indexing", 29, "i", "Ranked full-text index over artifacts."),
      S("SCC", "core/SCC-search.md", "search engine", 19, "i", "The backend search the overlay and tabs call."),
      S("DSL", "core/DSL-dynamic-skills-loading.md", "dynamic skills loading", 25, "d", "Loading skills on demand at run time."),
    ]),
    N("pf-runtime", "runtime services", "Progress, logging, notifications, plugins and small utilities.", [
      S("PRG", "core/PRG-progress-reporting.md", "progress reporting", 15, "v", "In-flight operations reported to the shell."),
      S("LGC", "core/LGC-logging.md", "logging", 24, "v", "Structured application logging."),
      S("NTD", "core/NTD-notification-delivery.md", "notification delivery", 22, "i", "Delivering notifications to the operating system."),
      S("PLG", "core/PLG-plugins-system.md", "plugins system", 10, "g", "Extension points for optional features."),
      S("FNT", "core/FNT-font-enumeration.md", "font enumeration", 10, "i", "Listing fonts installed on the host."),
      S("FGV", "core/FGV-flow-graph-validation.md", "flow graph validation", 15, "d", "Rejecting flow graphs that cannot run."),
      S("HIS", "core/HIS-history.md", "history", 9, "g", "Version history shared by several surfaces."),
    ]),
  ]),
  N("infra", "build & delivery", "Tasks, pipeline, container images and the server. Also the check that keeps specs referable.", [
    N("in-build", "build & ci", "Tasks, verification lanes, and the consistency check.", [
      S("TSK", "infra/TSK-taskfile.md", "taskfile", 31, "v", "Every command a person or agent runs."),
      S("CIP", "infra/CIP-ci-pipeline.md", "ci pipeline", 34, "v", "The lanes the pull-request gate runs."),
      S("SPC", "infra/SPC-specification-consistency.md", "spec consistency", 33, "v", "Checks every spec code and citation resolves."),
    ]),
    N("in-images", "agent images", "Pinned vendor containers and the mock used in tests.", [
      S("AVI", "infra/AVI-agent-vendor-images.md", "vendor images", 19, "i", "Container images pinned by digest."),
      S("ACM", "infra/ACM-agentic-cli-mock.md", "agentic cli mock", 30, "v", "Deterministic stand-in for a vendor CLI."),
      S("CCP", "infra/CCP-claude-code-cli-protocol.md", "claude code protocol", 28, "i", "Wire protocol spoken to one vendor CLI."),
      S("CDX", "infra/CDX-codex-cli-protocol.md", "codex protocol", 28, "i", "Wire protocol spoken to the other vendor CLI."),
    ]),
    N("in-server", "server", "The standalone Rust services shipped beside the desktop app.", [
      S("BMS", "server/BMS-backend-microservice.md", "backend microservice", 28, "d", "Standalone service and its health contract."),
      S("SAS", "server/SAS-server-application-service.md", "application service", 58, "d", "The server's application layer."),
      S("RSN", "server/RSN-remote-session.md", "remote session", 61, "d", "A desktop session reached through the server."),
      S("SRB", "server/SRB-server-relay-boundary.md", "relay boundary", 27, "i", "What the relay may and may not carry."),
      S("WSK", "server/WSK-websocket.md", "websocket", 47, "i", "The socket protocol between app and server."),
    ]),
  ]),
], "#8AB4E8");

export const DEMO_ROOTS: IndexNode[] = [authoring, graduation, agents, workspace, platform];
