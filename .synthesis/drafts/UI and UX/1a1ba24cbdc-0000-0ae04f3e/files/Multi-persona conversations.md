## Intent

IDE allows to have multiple agent personas, each of them can have its very own instructions. User should have a way to talk with them effectively all-at-once, and get their responses.

Like give user a way to hear more voices. To achieve this, one agent takes the temporary “Meta” role for each user message. Meta coordinates the other agents and writes the final reply. There is no hidden or extra participant: the Meta voice is always the voice of a real project agent.

The feature improves how the existing parallel flow (`@all` or several tags, AGT-FR-35..40) is shown and combined. It does not add a new way to address agents: typing tags stays the only way.

## User journey

### Single-agent conversation

In this situation, “advisors” become kind of very advanced tools for the primary, meta agent. Kind of “agent as a tool”.

- User has 3 agents defined in the project and 1 personal agent that’s not shadowed.

- User starts a conversation

- User tags specific agent with a question or statement or ask or whatever.

- Request is routed to the tagged agent, and tagged agent also becomes a temporary “Meta” agent.

- “Advisor” requests are routed in parallel to all other agents that are ready to answer, to “Advise”, based on their perspectives.

- “Advisors” do their own tool calls and thinking, providing the main agent “important points”, potential key questions etc. Kind of allowing the Meta agent to see the problem from different angles.

- “Meta” agent waits for its advisors with a fork-join (see Requirements: fork-join). It does not wait for all of them.

- “Meta” agent responds to the user like it does today. It can use the advice for more depth, and it can still do everything an agent does today: ask questions, advise the user, and propose changes. The user sees the pending row of the tagged agent only. That row shows tool calls as today, and a tool call made by an advisor names that advisor (see Requirements: progress).

- The reply carries a collapsed “Advisors” list. When the user expands it, the user sees each advisor’s important points and key questions.

### Multi-agent conversation

- User has 3 agents defined in the project and 1 personal agent that’s not shadowed.
- User starts a conversation.
- User tags several agents in one comment (or `@all`). Each tagged agent answers in its own pending row, in parallel, and its reply appears in the order it completes (as today).
- Before any tagged agent starts, a classifier step reads the discussed document and the instructions of the tagged agents. It finds the blast radius of the message and picks the most important tagged agent as the driver (Meta).
- When the fork-join ends, the driver posts one comment labelled “Synthesis”, in its own voice. The comment combines the replies, aligns them, and outlines the important points and open questions. The driver's final turn can also do everything an agent turn does today: ask questions, advise the user, and propose changes.
- Questions: if the agents ask questions, the user sees one merged question set (see Requirements).
- Changes: if the agents propose draft changes, the user reviews one merged proposal (see Requirements).

## Requirements

- **Switch.** One project setting, in the “Project Agents” tab of Project settings (SET-FR-15), turns the whole feature on or off: advisor mode (single tag) and Synthesis (several tags). It is optional and off by default. The switch and the timeout (see Fork-join) are stored per project on this machine only. They are never committed with the project. With the switch off, behaviour is exactly as today.
- **Ready agent.** An agent is ready when the user could ask it now: it is not shadowed, it has a model defined, and it can run. Only ready agents are dispatched as advisors, and only ready tagged agents count toward the quorum.
- **Progress.** There is no new progress surface. The pending row of the tagged agent (single tag) or of each tagged agent (several tags) shows tool calls as today (CTA-FR-QXIG). In a single-tag conversation it also shows the tool calls of the advisors. Each of those names the advisor that requested it.
- **Single agent.** If only one agent is dispatched (one tag and no other ready agent, or `@all` with one ready agent), there is no quorum, no advisor, no classifier and no Synthesis. It is a regular turn, bound by the regular turn timeout (CVL-FR-16). The fork-join timeout does not apply.
- **Fork-join.** Advisors (single tag) and tagged agents (several tags) run in parallel. The **timeout** starts when a **quorum** (a majority of the dispatched agents) has finished. The join ends when all dispatched agents have finished, or when the timeout ends, whichever is first. The timeout is a project setting in the same tab, with a default of 60 s. A failed or cancelled agent is skipped and does not count toward the quorum. Turns that are not finished when the join ends are cancelled and marked as skipped. If no agent finished, Meta answers alone (single tag) or no Synthesis is posted (several tags).
- **Meta for several tags.** The classifier is an LLM-driven step. It runs once per message, before any tagged agent starts, so the driver is known at dispatch. It picks the driver from the tagged agents only, using the document under discussion and the tagged agents’ instructions. It runs on the model and reasoning that the user selected in the AI API section of Global settings, on the provider that the open project resolves to. It is not an agent turn, so no agent’s own model is used. If the classifier cannot run, no driver is picked, no Synthesis is posted, and the message follows the parallel flow as today. The driver writes the “Synthesis” comment. It is a normal comment, in the driver’s voice, labelled “Synthesis”.
- **Advisors list.** In a single-tag conversation, Meta’s reply has a collapsed “Advisors” list. It shows each advisor that finished, and the skipped ones with the reason. The Advisors list and the “Synthesis” label are stored with the comment, so they show again when the conversation is opened later.
- **Merged questions.** In a multi-agent conversation, the agents’ questions are deduplicated and ranked by the driver into one pending question set of at most ten questions (ADQ-FR-GMDK; DQA). Agents do not record their own set.
- **Merged proposals.** In a multi-agent conversation on a draft, the driver rewrites the agents’ proposed changes into one proposal with non-overlapping hunks. The author reviews it as one proposal (DCP, DCR).
- **Specifications this work changes.** `AGC-agent-conversations.md` (AGC-FR-01 and AGC-FR-04: today a turn is independent and nothing merges; the tool-call record must also carry the advisor that made a call); `CTA-comment-agent-turns.md` (dispatch, pending rows, and advisor tool calls that name the advisor); `CVP-conversation-presentation.md` (Synthesis comment and Advisors list); `AGT-agents.md`; `SET-project-settings.md` (Project Agents tab); `ADQ-ask-discussion-questions-tool.md` and `DQA-discussion-question-answering.md` (merged set); `DCP-draft-change-proposals.md` and `DCR-draft-change-review.md` (merged proposal); `CVL-conversation-loop.md` (advisor turns, classifier step); `CMS-comments-storage.md` (the Advisors list and the Synthesis label stored with a comment); `PSS-project-settings-storage.md` and `GSS-global-settings-storage.md` (a per-project, machine-only home for the switch and the timeout; today neither holds them). `AAP-ai-api-integrations.md` and `AII-ai-integrations.md` (a new reader of the stored AI API model: the driver classifier; today only the graduation loop reads it).

## Expected deliverables

- Project Agents tab in Project settings has the on/off switch (default off) and the join timeout field (default 60 s).
- Storage for the switch and the timeout: per project, on this machine only, never committed.
- Fork-join dispatcher: runs advisors or tagged agents in parallel, starts the timeout at majority quorum, ends when all finish or the timeout ends, cancels and marks the rest as skipped.
- Single tag: the tagged agent becomes Meta, and the other ready agents run as advisors (an “agent as a tool” call that returns important points and key questions).
- Driver classifier tool: an LLM step on the user-selected AI API model. It picks the driver from the tagged agents using the document and their instructions.
- “Synthesis” comment: a normal comment in the driver’s voice, with the label stored in the comment. The classifier runs before the agents start, and its failure means no Synthesis and today's behaviour.
- Collapsed “Advisors” list on Meta’s reply, stored in the comment: finished advisors, and skipped ones with the reason.
- Merged question set: deduplicated and ranked by the driver, at most ten questions, and agents do not record their own set.
- Merged draft proposal: the driver rewrites the agents’ changes into one proposal with non-overlapping hunks.
- Pending rows and error states for advisor turns and for skipped turns.
- Tool-call progress in the pending row, with the advisor's name on each advisor tool call.
- Ready-agent check, and the regular-turn path when only one agent is dispatched.
- Specifications updated: all those listed in Requirements.
- Tests for quorum, timeout, failed and cancelled agents, the switch off (behaviour as today), a single dispatched agent (regular turn, regular timeout), ready-agent filtering, and the merged questions and proposals.