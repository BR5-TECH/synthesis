# Discussion question answering

**Spec code:** `DQA`

## Intent
Where the author answers a discussion agent that has asked several things at once. An agent working in a discussion records a **pending question set** against it (per `../tools/ADQ-ask-discussion-questions-tool.md` ADQ-FR-GMDK), and this is the block that renders those questions above the discussion's composer and takes the answers back. It is answered one question at a time with pagination in both directions, because a set of questions presented at once is a form rather than a conversation, and because an author who has answered six of seven should see which one is still open. Each question offers two or three proposed options and one more row for the author's own answer, so a set never holds the author to an answer they do not mean. Exactly one answer stands per question, none is chosen for them, and a chosen option takes a note beside it. Until the author submits, the block is **outside** the discussion's history and the discussion takes no other contribution — a conversation cannot be answered around a question it is holding. On submission the whole set lands in the conversation as prose, each question and its answer read as one exchange, and the discussion carries on. The shape follows `GEA-graduation-escalation-answering.md`'s deliberately: an author who has answered a graduation run's questions already knows how to answer these. Out of scope: the tool that records the set, the record's shape, its persistence, and the rules that keep it single and idempotent, all of which are `../tools/ADQ-ask-discussion-questions-tool.md`'s and `../core/CMS-comments-storage.md`'s; graduation escalation answering, which is `GEA-graduation-escalation-answering.md`'s separate surface against a run and is unchanged by any of this; the open single question with no options, which arrives as an ordinary comment and is answered by writing one (per `../tools/AUC-ask-user-comment-tool.md`); where a discussion is rendered, which is `CVP-conversation-presentation.md`'s and `DDS-draft-discussion.md`'s; and who the answers reach, which is the discussion's ordinary routing (per `CTA-comment-agent-turns.md` CTA-FR-PXJB).

## User stories
- As an author whose agent found three open points in a draft, I want to answer all three in one sitting, so the discussion moves in one round rather than three.
- As an author part-way through a set, I want to page back to what I already chose and change it, so a set of questions is a decision rather than a quiz.
- As an author who closed the application overnight, I want the questions still waiting, so an interruption costs me the answers rather than the questions.
- As an author whose submission was refused, I want everything I entered still there, so I correct one thing rather than typing the set again.

## Wireframes

The block, above the composer of a discussion:

```
┌──────────────────────────────────────────────────────────────┐
│  @arch  ·  Architect                                         │
│  Question 2 of 3 · 1 answered · 2 still need an answer       │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   Should the ontology live in one spec or two?               │
│                                                              │
│    ( ) one — a single spec covering both layers              │
│    (•) two — paired ui/ and core/ specs                      │
│    ( ) Something else — I will say                           │
│                                                              │
│    Add a note…                                               │
│    ──────────────────────────────────────────────────────    │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│  [ ‹ Previous ]        ● ○ ●        [ Next › ]  [ Submit ]   │
└──────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│  Answer the questions above to continue the discussion.      │
└──────────────────────────────────────────────────────────────┘
```

The same question, answered in the author's own words. The own answer opens a field of its own, and no note is offered beside it:

```
│                                                              │
│   Should the ontology live in one spec or two?               │
│                                                              │
│    ( ) one — a single spec covering both layers              │
│    ( ) two — paired ui/ and core/ specs                      │
│    (•) Something else — I will say                           │
│    ┌────────────────────────────────────────────────────┐    │
│    │ three — one per layer and one for the join         │    │
│    └────────────────────────────────────────────────────┘    │
│                                                              │
```

A submitted set, read back in the discussion's history as three paired entries:

```
│ @arch                                            Architect   │
│ Should the ontology live in one spec or two?                 │
│   1. one — a single spec covering both layers                │
│   2. two — paired ui/ and core/ specs                        │
│ ─────────────────────────────────────────────────────────    │
│ @raver119                                              now   │
│ **Selected option:** two — paired ui/ and core/ specs        │
│ **Note:** keep the flow diagram in the ui one.               │
```

An entry the author answered in their own words:

```
│ @arch                                            Architect   │
│ Should the ontology live in one spec or two?                 │
│   1. one — a single spec covering both layers                │
│   2. two — paired ui/ and core/ specs                        │
│ ─────────────────────────────────────────────────────────    │
│ @raver119                                              now   │
│ **Own answer:** three — one per layer and one for the join   │
```

The same two entries in the draft's discussion column, each as one question card:

```
┌──────────────────────────────────────────────────────────────┐
│ QST  @arch                                             now   │
│ Should the ontology live in one spec or two?                 │
│    1  one — a single spec covering both layers               │
│ ▌  2  two — paired ui/ and core/ specs                       │
├──────────────────────────────────────────────────────────────┤
│ YOUR NOTE   keep the flow diagram in the ui one.             │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│ QST  @arch                                             now   │
│ Should the ontology live in one spec or two?                 │
│    1  one — a single spec covering both layers               │
│    2  two — paired ui/ and core/ specs                       │
├──────────────────────────────────────────────────────────────┤
│ YOUR ANSWER  three — one per layer and one for the join      │
└──────────────────────────────────────────────────────────────┘
```

- Layout notes: the block sits between the conversation's message list and its composer, inside whichever surface renders that discussion, and it is no floating overlay and no modal — the rest of the window stays operable while it stands. It carries a background of its own, distinct from the surface behind it; that background is white in neither theme, except in the draft's discussion column, where it is the canvas tone every card there takes. Its head names the agent that asked and its title where the participant carries one, and states which question of how many is showing and how many still need an answer. Its body is one question at a time: the question text, then the recorded options and the own answer as rows of one answer group, and below them the note of that question, which is a field marked by its placeholder alone. The own answer opens a field of its own, and a question answered in it offers no note. Its foot carries Previous, a position indicator, Next, and Submit on one line; Previous and Next disable themselves at the two ends of the set. The block is set on the measure of the conversation it stands in, and is centred within its surface rather than filled to that surface's width. The composer below it renders disabled with one line of text saying why. In history, a question comment and the answer comment immediately after it are drawn as one entry rather than as two stacked cards: a single rule between the two halves in every owner surface other than the draft's discussion column, and one question card in the draft's discussion column, where the chosen option is marked in the option list and the note stands in the card's footer.

## UI contract boundary
- **Owned by the UI**: the block's placement, measure, and treatment in every owner surface that renders it; the rendering of every recorded question in its recorded order with its recorded options; the answer rows of one question, the own-answer field, and the note beneath them; what counts as answered; the pagination and what it moves; the enablement of Submit and the reason it states while unavailable; the disabling of the composer and of every other human-authored contribution route while a set stands; the unsent answer draft — what it is keyed by, what it holds, what it survives, and when it is dropped; the single routing decision the accepted submission takes and the dispatch it performs; and the pairing of a question comment with the answer comment after it into one history entry.
- **Delegated to backend (abstract)**: `"read discussion question set"` and `"submit discussion question answers"` — both owned by `../core/CMS-comments-storage.md` (CMS-FR-JWVH, CMS-FR-TXRB) — and the event `"discussion question set changed"` (CMS-FR-BQEN), which is how a surface learns a set appeared or went without asking again. `"dispatch agent turn"` — owned by `../core/AGC-agent-conversations.md` — is what an accepted submission invokes afterwards, on exactly the terms `CTA-comment-agent-turns.md` invokes it (CTA-FR-PXJB). This surface introduces no other operation, and it never invokes `"add comment to discussion (discussion, body, quotes, attachments)"` for an answer: the whole submission is one backend operation.

## Functional requirements
 1. **DQA-FR-NRZB** A discussion holding a pending question set renders it as a **question block** above the composer, in **every** owner surface that renders the shared discussion surface (per `CVP-conversation-presentation.md`): the Editor rail's Discussion section (per `CMT-comments.md`), the action control's floating discussion panel (per `ACT-action-control.md` ACT-FR-20), the Flow, Diff, and History panels, the conversation tab, and the draft's discussion column (per `DDS-draft-discussion.md` DDS-FR-QZAV). The block and its behaviour are identical in each, whatever the discussion's target and whether or not it carries a fragment.
 2. **DQA-FR-TVMH** The block is **outside the discussion's history** until submission. It contributes no comment, occupies no position in the message list, is quotable by nothing, is counted by no unresolved-discussion total, and no reader of the conversation sees it.
 3. **DQA-FR-CKYP** It is an in-panel block and never a floating overlay of the window and never an application modal (per `SNV-shell-navigation.md` SNV-FR-56). It takes no focus trap, and the rest of the window stays operable while it stands.
 4. **DQA-FR-JWEF** A surface reads the set from `"read discussion question set"` when it mounts the discussion and follows `"discussion question set changed"` thereafter, so a set recorded while the author is reading appears without a reopen and one submitted elsewhere goes.
 5. **DQA-FR-XQOR** It renders every recorded question in its recorded order with its recorded text and its recorded options, exactly as they were recorded, and composes no prose of its own around them.
 6. **DQA-FR-PDLN** It names the agent that asked, from the set's own recorded participant snapshot, and renders that participant's title on the terms every agent contribution renders one (per `CTA-comment-agent-turns.md` CTA-FR-KFUF, CTA-FR-KYPK).
 7. **DQA-FR-BXHU** The set is answered **one question at a time**, with pagination in both directions, the author moving forward and backward through every recorded question.
 8. **DQA-FR-GVSA** A set the author has not paged shows its **first** question, and a set that replaces another does the same, a page belonging to the set it was turned in.
 9. **DQA-FR-ZLKD** Moving between questions invokes nothing, and a question is never skipped, reordered, or hidden because another is unanswered.
10. **DQA-FR-TFCE** Moving takes the keyboard with it: focus lands in the question that arrived rather than staying on the control that moved it, because Previous and Next disable themselves at the two ends of the set.
11. **DQA-FR-MJPV** The block states which question of how many is showing and how many of the set still need an answer, so the author knows what is left without paging through to find out.
12. **DQA-FR-QWTB** Each question renders its **two or three** recorded options as rows of one answer group, each row reading as that option's recorded value, and **no option is preselected**.
13. **DQA-FR-FCZL** Each question's answer group carries an **own answer** row after its recorded options, and choosing that row opens a field for the author's own words. The row stands on every question, and the agent that asked cannot withhold it.
    - *Why:* An agent cannot tell which of its proposed options will miss, and a standing set closes every other route into the discussion.
14. **DQA-FR-HLDS** Exactly one answer stands per question — one recorded option, or the own answer. Choosing another replaces the one before it, and there is no way to leave a question with two answers chosen.
15. **DQA-FR-ROXG** Each question carries a **dedicated note** of its own, below that question's answer rows and belonging to that question alone. Its **placeholder** is what says the note is optional and what it is for, and no control opens it.
16. **DQA-FR-JJON** The note is offered only where a **recorded option** is selected. A question answered in the author's own words offers no note and sends none, and note text already written stays in the draft and returns with the option the author chooses.
    - *Why:* The author's own words already say what they mean, so a note beside them asks the same thing twice.
17. **DQA-FR-UAKC** The note is **not** an answer row and never stands in for a choice: typing a note selects nothing, and a question carrying a note and no answer is unanswered.
    - *Why:* The own answer row is how the author says something else, so a note stays an addition to an answer rather than a way of giving one.
18. **DQA-FR-SEBN** A question is **answered** when one of its recorded options is selected, or when the own answer is selected and its text is non-blank once trimmed. A note answers nothing.
19. **DQA-FR-WGQY** **Submit** is enabled only once **every** recorded question is answered, and the block states how many still need an answer while any do, so a disabled control is never a mystery.
20. **DQA-FR-KDVU** Submitting invokes `"submit discussion question answers"` **once** with the set's id and the whole ordered set of answers. Each entry names the question's recorded position, and then either the chosen option's recorded position, that option's recorded value, and the note where one was written, or the author's own words — as separate fields.
21. **DQA-FR-GOZY** The block is answerable under the same identity rule as every composer (per `CMT-comments.md` CMT-FR-24, CMT-FR-26): a project that stores no token submits its answers as **Me** with no picker and no missing-token error, and the answer comments display under `CMT-comments.md` CMT-FR-ZCAE. A refused identity keeps the submission refused with its typed error and every entry intact (DQA-FR-WKTP).
21. **DQA-FR-AYNP** Submit is **single-flight**: it disables itself the instant it is activated and every further activation is ignored until the request settles, so one press is one submission.
22. **DQA-FR-WKTP** A submission the backend **refuses** keeps the block, every selected answer, every own answer's text, and every note exactly as the author left them, and renders the typed error inline at the foot of the block. The author corrects what was refused rather than answering the set again.
23. **DQA-FR-OMZL** Until the backend has accepted the set, no part of this surface shows the discussion as answered, and the composer stays disabled.
24. **DQA-FR-IPFD** An **accepted** submission removes the block and its unsent answer draft together, and the discussion's composer and its other contribution routes become available again.
25. **DQA-FR-PXNC** While a set stands, the discussion's **composer is disabled** and carries one line of text saying that the questions above are to be answered first. It states nothing about the agent, about a turn, or about how long an answer has been owed.
26. **DQA-FR-VJHT** While a set stands, **no other human-authored contribution** to that discussion is offered: its composer posts nothing, its Quote action seeds nothing, and any other control that would append a human-authored comment to it is disabled. Nothing here disables reading, scrolling, locking, resolving, or moving the conversation between owner surfaces.
27. **DQA-FR-EGZS** The unsent **answer draft** is what the author has entered and not yet sent. It holds, per question, the answer selected, the author's own words, and the note text, and nothing short of an accepted submission replaces it.
28. **DQA-FR-CBQK** The draft is keyed by the **set's id** together with each question's **recorded position** — never by the question's text and never by the index of the page showing.
29. **DQA-FR-LSNW** The draft survives paging between questions, a re-read of the set, a re-render, the author moving the discussion between owner surfaces and conversation tabs, switching tabs and coming back, and a backend refusal of the whole set.
30. **DQA-FR-DYFR** A set re-read while the block stands is **reconciled rather than replaced**: the questions and their options are redrawn from what came back, every draft entry whose position the re-read set still records is kept against that position, and an entry for a position the set no longer records is dropped with it.
31. **DQA-FR-HCTM** The draft is discarded when the set it belongs to is submitted and accepted, and when the discussion stops holding that set by any other route.
32. **DQA-FR-ZUPB** The draft is held in memory for the life of the running application, is written to no preference and to no backend operation, and no draft of one set is ever read for another. A restart therefore finds the questions and none of the answers (per `../tools/ADQ-ask-discussion-questions-tool.md` ADQ-FR-PZWD).
33. **DQA-FR-NKAX** An accepted submission's appended comments arrive in the conversation through the ordinary `"discussion changed"` event and are ordinary comments of it thereafter — stacked in order, quotable, immutable, and counted like any other (per `CMT-comments.md` CMT-FR-08, CMT-FR-12, CMT-FR-14).
34. **DQA-FR-FBWO** A **question** comment immediately followed by an **answer** comment renders as **one history entry** rather than as two stacked cards. Outside the draft's discussion column that entry shows the question and its options above and the answer below, under one heading naming the agent that asked and the author that answered (per `../tools/ADQ-ask-discussion-questions-tool.md` ADQ-FR-RECR).
35. **DQA-FR-GRUV** The pairing is **adjacency-only** and never skips another comment: a question comment with any other comment between it and the next answer comment renders as an ordinary comment on its own, and so does the answer.
36. **DQA-FR-TSJD** The pairing is presentation alone. No stored relation joins the two comments, neither body is rewritten to render them together, and a surface that draws them separately loses nothing but the grouping.
37. **DQA-FR-KYWR** In the **draft's discussion column** the entry renders as **one question card**: the question text, its recorded options, the chosen option marked in the option list, and the note in the card's footer. No line of that transcript states the answer a second time (per `DDS-draft-discussion.md` DDS-FR-BRHN).
    - *Why:* An answer repeated as its own message doubles the length of every exchange the author already decided.
38. **DQA-FR-ZPGM** Where the author answered in their **own words**, that card marks no option and carries those words in its footer under the label `your answer`, in the place and the shape a note takes.
39. **DQA-FR-BHXT** An answered question card is **read-only**. It offers no control that changes the answer and no control that reopens the question.
40. **DQA-FR-CIRK** An accepted submission is one **routing batch**. The surface resolves the discussion's active agents **once**, after the append has committed, and dispatches exactly **one** fresh turn per active agent for the whole submission, naming the **final answer comment** as each turn's trigger (per `CTA-comment-agent-turns.md` CTA-FR-PXJB).
41. **DQA-FR-QMEA** No individual question or answer comment dispatches a turn of its own, so a set of ten questions costs one turn per active agent rather than ten.
42. **DQA-FR-XBTL** A dispatch the backend refuses follows the ordinary dispatch-refusal handling (per `CTA-comment-agent-turns.md` CTA-FR-UUXA) and **rolls nothing back**: the comments are committed, the set is gone, and the author retries the dispatch rather than the answers.
43. **DQA-FR-VNKQ** The block's question area carries a background of its own, distinct from the surface behind it. Outside the draft's discussion column that background is white in neither theme; in that column it is the canvas tone every card there takes (per `DDS-draft-discussion.md` DDS-FR-BRHN). Every text in it keeps its contrast in each theme setting (per `GLS-global-settings.md` GLS-FR-04).
44. **DQA-FR-RDPU** The block is set on the same **measure** as the conversation it stands in, and is centred within its surface rather than filled to that surface's width.
    - *Why:* The block and the messages above it are one conversation, so a measure of the block's own reads as a narrower thing floating inside the transcript.
45. **DQA-FR-EWLB** Every control this surface introduces — each answer row, each own-answer field, each note field, Previous, Next, and Submit — is reachable and operable from the keyboard alone and carries the name it would have spelled out, and every state it introduces is carried by more than colour.
46. **DQA-FR-MQZE** The arrival of a set, a move between questions, an accepted submission, and a refusal are each **announced** accessibly, naming the discussion and what changed.
47. **DQA-FR-JADB** The block and the question card render each question's text and each option with every provider citation marker hidden (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU). The recorded set, and the answer a submission sends, are unchanged by it.
48. **DQA-FR-HMTR** The arrival or replacement of a set follows the unread and scroll-follow rule of the shared discussion surface, in every owner surface. While the message list is at its end, the block stands in view and the list keeps following. While the author has scrolled away from the end, the arrival moves nothing: the surface shows its unread indicator, which counts the set as one unread item, and the jump control brings the author to the block. A set that arrives while the discussion is not shown raises the discussion's unread state, which the surface shows when it is next shown. The author's answer draft, scroll position, and unread state are kept in session stores keyed by discussion id.

## Non-functional requirements
- The block holds no state that survives the application. What survives is the set itself, on its own persistence contract, so a relaunch loses only what was never sent.
- The whole block is operable from the keyboard alone: paging, choosing an answer, writing an own answer, typing a note, and submitting are all reachable, and focus is never trapped inside it.
- Every text in the block meets the contrast floor in all three theme settings, and the type size follows the reader's chosen font.
- Answering is never blocked by a slow read: the questions render from the set the surface already holds, and the one call a submission makes is the submission itself.
- A submission of ten questions is one backend call and one routing decision, so the cost of answering a large set is the cost of answering a small one.
