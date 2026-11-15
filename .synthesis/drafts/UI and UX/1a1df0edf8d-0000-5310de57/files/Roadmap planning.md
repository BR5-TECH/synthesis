## Intent

User should have an ability to use conversatonal agents for project planning activities using discussions in Map. User should be able to drive the development from HLD down to milestones and features and actual drafts/tasks that implement those. Providing a basic SDLC support inside the IDE.

Users: authors and architects who plan delivery. Scope of v1: discussions in the Map tab only, and GitHub issue tools in Map discussions only. Draft and file (HLD) discussions do not get the tools in v1.

## User journey

User working on the HLD document, or on a draft can affect delivery roadmap we have.

User working on the delivery planning should be able to task agent with planning-related tasks.

User should be able to use conversational agent for gaps analysis between what he has in HLD and in the github project features. 

- User opens the Map tab. A **Discussion column** shows the Map discussion. There is one Map thread per project. By default it covers the whole map.
- User can mention a node (for example a feature) in the Map thread. The node is then the focus of that turn. User can also start a new thread about one node.
- User right-clicks a node or a milestone on the Map canvas. The context menu has a **Discuss** entry. It opens the Discussion column with a new thread focused on that node or milestone. Where a thread for that item already exists, the entry opens it and creates no second thread.
- User asks the agent for a gap analysis. User names the HLD file in the message. The agent reads the HLD and the plan issues, then replies with the gaps. For each gap the agent proposes an issue. Where a feature exists but is not fully implemented, the agent can propose a new draft instead of an issue.
- User asks the agent to change plan issues (title, body, Type, milestone, parent, labels, close, Status).
- Each proposal shows on one review card in the thread. User accepts or rejects each item on the card. Nothing reaches GitHub, and no draft is created, before the user accepts that item.

## Requirements

- **Map discussion.** The Map tab gets a Discussion column. One Map thread exists per project and covers the whole map. A message can mention a node as focus. User can start a separate thread about a node. The context menu of a node and of a milestone has a **Discuss** entry that opens the Discussion column and starts that thread, or opens the existing one. The entry is available on Delivery lens nodes, Delivery lens milestones, and Specifications lens nodes. A milestone can also be the focus of a mention. The entry has a keyboard route and the same icon rule as other context menus (`LCM-library-context-menu.md` LCM-FR-07). The column follows the discussion rules of `CMT-comments.md` and `DDS-draft-discussion.md`.
- **Availability.** The GitHub issue tools are attached only to turns of Map discussions. They are attached only while a GitHub Project is selected in Project settings (GPP-FR-NLPG). Otherwise the agent does not see them.
- **Built-in tools.** The tools are built into the application and reuse the existing GitHub integration: the project token (GTS), the selected Project, and the plan read and write path of `GDP-github-delivery-plan.md`. No GitHub MCP server and no second token flow.
- **Read tools** list, search, and read plan issues only: the issues of the selected Project that are part of the plan (DSR-FR-MKIG, DSR-FR-TZDZ). They reach no other issue of the repository.
- **Write tools** only propose. A create or modify call records a proposal and writes nothing to GitHub. A new issue can set title, body, Type, milestone, parent, and labels. A modify proposal can change title, body, Type, milestone, parent, labels, close, and the Project Status.
- **Draft proposal.** The agent can propose a new draft in place of an issue. Accepting it creates the draft and writes nothing to GitHub.
- **Review card.** One agent comment carries one card with one row per proposed issue, issue change, or draft. A modify row shows each changed field as before and after. Each row has its own **Accept** and **Reject**, a state (`Waiting for you`, `Accepted`, `Rejected`, or failed), and its typed error where the write fails. A decision applies to one row only. A failed row does not change other rows. Every control works by keyboard alone.
- **Gap analysis.** The reply states each gap between the HLD and the plan issues. Each gap has a matching proposal row where the agent can propose one.
- **Stale writes.** Where an issue changed on GitHub after the proposal, accept fails with `changed_on_tracker` (GDP). The row shows the error and changes nothing.
- **Batch proposal.** One propose call carries many rows (create issue, modify issue, new draft) and fills one card. The agent does not need to propose all work at once. It keeps the discussion focused and proposes in steps.
- **Row dependency.** A row can refer to another row on the same card, for example a new issue whose parent is a new issue that still waits. The child row shows that it waits for its parent. User can accept the child only after the parent is accepted and created. Then the child uses the real parent issue. Where the parent is rejected or failed, the child cannot be accepted and the row shows the reason.
- **Labels.** A modify row changes labels with an add list and a remove list. Only labels that already exist in the repository are allowed. The row shows each added and removed label. A name that does not exist fails the proposal with a typed error. The tool creates no label.
- **Close.** A close row carries a close reason that the agent sets (for example `completed` or `not_planned`). The row shows the reason to the user before accept.
- **One open card.** In one Map thread the agent cannot record a new proposal while a card of that thread has a row in state `Waiting for you`. The propose call fails with a typed error that tells the agent to wait for the decisions. The agent can propose again when every row is decided. A decided or failed row does not block.

## Expected deliverables

- Discussion column and Map discussion thread (and node threads) in the Map tab.
- **Discuss** entry in the context menu of Map nodes and milestones.
- Built-in agent tools for Map discussions: list, search, and read plan issues; one batch propose tool whose rows are create issue, modify issue, or new draft.
- Proposal review card with per-row accept and reject.
- Backend path that applies an accepted row through the existing GitHub integration, including labels, close, and Status.

## Specifications to update

- `specifications/ui/SMP-specification-map.md`: Discussion column in the Map tab and the Map session.
- `specifications/ui/MDL-map-delivery-lens.md` and `specifications/ui/SMN-specification-map-nodes.md`: the **Discuss** context-menu entry on milestones and nodes.
- `specifications/ui/CMT-comments.md` and `specifications/core/CMS-comments-storage.md`: the Map discussion target and the node reference of a thread.
- `specifications/core/GDP-github-delivery-plan.md`: the edit set must cover labels, close, and Status writes, which are out of scope today.
- `specifications/tools/`: new specifications for the issue tools, in the shape of `PND-propose-new-draft-tool.md`, and `TLC-tool-conventions.md` for the tools that declare a side effect.
- `specifications/ui/DCR-draft-change-review.md`: the pattern of the review card; a new Map proposal review specification.