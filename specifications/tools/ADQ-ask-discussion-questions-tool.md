# Ask discussion questions tool

**Spec code:** `ADQ`

## Intent
The tool a discussion agent reaches for when several things are unsettled at once. A discussion is where material is decided rather than corrected, and an agent that has read a draft usually finds two or three open points instead of one; asking them one at a time costs the author one round trip each and costs the agent the answers it needed together. This tool puts the whole set at once. One call records a **pending question set** against the discussion — an ordered list of up to ten questions, each carrying two or three proposed options — and ends the agent's turn there. The set is not a comment: it is a durable record the author answers in a block above the discussion's composer, question by question, choosing one option for each or answering in their own words, and adding a note to a chosen option where they want one. Only when the author submits does the conversation gain lines, and it gains them as prose: one agent-authored question comment and one human-authored answer comment for each question, in the order the questions were asked. The set is persisted per discussion and survives an application restart, because the agent turn that asked does not — an author who closed the application mid-decision comes back to the questions rather than to nothing. Out of scope: an **open** question with no proposed options, which is `AUC-ask-user-comment-tool.md`'s and stays available; asking anything outside a discussion, since this tool is offered to discussion turns alone and to no anchored comment thread and no graduation turn; the graduation escalation, which is `ESU-escalate-to-user-tool.md`'s separate contract against a run and is unchanged by any of this; rendering the block, paging it, and collecting the answers, all of which are `../ui/DQA-discussion-question-answering.md`'s; delivering the answers, which is a fresh later turn the discussion's own routing dispatches; and waiting for the author, since the call returns as soon as the set is recorded and holds no timer, watcher, or open request.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to a **discussion** turn alone by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own. The durable record it creates and the operations that read, submit, and delete it are `../core/CMS-comments-storage.md`'s (CMS-FR-QLDW, CMS-FR-TXRB, CMS-FR-JWVH).

### The tool
`ask_discussion_questions` — a `rig` portable tool. It is **constructed per turn** (per TLC-FR-15), its constructor taking the discussion the turn belongs to and the agent participant that turn answers as, so the discussion it can record against is fixed before the model composes a single argument.

### The description
The fixed text the model reads (per TLC-FR-05):

> Put several questions to the author at once, when more than one thing about this discussion is unsettled and you can propose the likely answers to each. Ask up to ten, each with two or three options the author picks between. Reach for it instead of guessing and instead of answering every reading in turn. Your questions are recorded against this discussion and your turn ends there — this call brings you no answer back, and you will be asked again later with every question and the author's chosen answers in front of you. Ask everything you are stuck on here: anything else you asked for in the same message is not carried out. Use it only where you can propose options; for one open question with no options to offer, use `ask_user_comment` instead. It records against the discussion you were asked in and nowhere else, and it changes no file of the project.

### Arguments
```
AskDiscussionQuestionsArgs {
  questions: [           // required; 1 to 10, in the order they are to be answered
    {
      question: string,  // required; one thing asked
      options:  string[] // required; 2 to 3 proposed answers, in order
    }
  ]
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `questions` — *"Everything about this discussion you are stuck on, between one and ten entries, in the order you want them answered. Ask them all here: your turn ends on this call, so a thing you leave out is a thing you cannot ask about later."*
- `questions[].question` — *"One question, written as you would put it to a colleague. Ask one thing; a question with three parts comes back with one answer to whichever part they read last."*
- `questions[].options` — *"Two or three answers the author picks between, each a short phrase. Every question needs them. Where you have none to propose, do not use this tool: ask that one question with `ask_user_comment` instead."*

### Output
Data (per TLC-FR-08): `{ recorded: true }`. The turn ends on the call that recorded (per `../ai/CVL-conversation-loop.md` CVL-FR-15), so no further model call reads this value.

### The pending question set
The durable record one successful call creates. It is keyed by the discussion's own thread id, holds one set per discussion, and is what the author answers:

```
PendingQuestionSet {
  set_id,                  // opaque, unique within the project; the idempotency
                           //   identity of the submission transaction (ADQ-FR-YQTB)
  thread_id,               // the discussion this set belongs to; the record's key
  asked_by: Participant,   // the asking agent's full participant snapshot, agent kind
  asked_at,                // RFC 3339 UTC
  questions: [
    {
      position,            // 1-based, stable, ascending; assigned by the application
      text,
      options: [ { position, value } ]   // 2 to 3, 1-based, stable, ascending
    }
  ]
}
```

It carries **no** agent-turn id, no model exchange, no selected option, no note, and no presentation state. The participant snapshot is what the generated question comments are stamped with, after an application restart as before one (ADQ-FR-PZWD).

### The submitted answers
What the author sends back, one entry per recorded question, in the recorded order:

```
QuestionAnswer {
  question_position,       // the recorded position of the question this answers
  option_position?,        // the recorded position of the option chosen
  option_value?,           // that option's recorded value
  own_answer?,             // the author's own words, where no option was chosen
  note?                    // the author's addition to a chosen option, absent where none
}
```

### The comments a submission appends
Two comments per question, in the recorded question order, question first (ADQ-FR-MRSK). The **question** comment is agent-authored and carries the recorded question text, then its recorded options as an ordered numbered Markdown list:

```
Should the ontology live in one spec or two?

1. one — a single spec covering both layers
2. two — paired ui/ and core/ specs
```

The **answer** comment is human-authored and carries one line, and a second paragraph where the note is non-blank:

```
**Selected option:** two — paired ui/ and core/ specs

**Note:** keep the flow diagram in the ui one.
```

An answer the author wrote in their own words carries the other opening line and no note:

```
**Own answer:** three — one per layer and one for the join
```

Nothing else is added to either body: no closing line, no routing tag, no identifier, and no machine-readable marker.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **already pending** — kind `PermissionDenied`, **not** retryable: *"This discussion already has a question set waiting on the author, so nothing was recorded. Answer with what you already have; asking again will not succeed."*
- **no questions** — kind `InvalidArgs`, retryable: names that between one and ten questions are accepted and how many were supplied.
- **too many questions** — kind `InvalidArgs`, retryable, on the same terms.
- **blank question** — kind `InvalidArgs`, retryable: names which question's text was blank and what it is for.
- **wrong number of options** — kind `InvalidArgs`, retryable: names which question carried how many, and that two or three are accepted.
- **blank option** — kind `InvalidArgs`, retryable: names which question's option was blank.
- **too long** — kind `InvalidArgs`, retryable: names which value exceeded which limit.
- **conversation locked** — kind `PermissionDenied`, not retryable: *"This conversation is locked and takes no further contribution, so nothing was recorded and no answer is coming."*
- **could not record** — kind `Other`, retryable: *"The questions could not be recorded against this discussion."*

### Side effects
Declared in full (per TLC-FR-17). One successful call creates **exactly one** `PendingQuestionSet` for the discussion the turn belongs to, through `../core/CMS-comments-storage.md`'s reservation path (CMS-FR-QLDW). It **appends no comment**. Nothing else changes anywhere: no file of the project is created, modified, or deleted, no comment log gains a line, and no other store is mutated. A refused call records nothing.

## Functional requirements
 1. **ADQ-FR-KZNV** The tool exists as a `rig` portable tool named `ask_discussion_questions` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, and logging are that spec's rules applied to this tool. It declares a side effect under TLC-FR-17, and that side effect is the contract surface's and nothing beyond it.
 2. **ADQ-FR-WBQL** Its `description()` returns the fixed text of the contract surface, compiled into the binary, and its `parameters()` returns the JSON Schema for the one documented argument with the documented parameter descriptions (per TLC-FR-05). Neither varies by discussion, by agent, or by project.
 3. **ADQ-FR-HTGC** The tool records against the discussion of the turn that holds it and accepts no argument naming a conversation, a thread, a target, or a participant. Its constructor binds it to that turn's origin and agent participant (per TLC-FR-15), so no argument a model composes can reach a conversation it was not asked in, and an instance outlives no turn.
 4. **ADQ-FR-LFDX** The tool is offered to turns on a discussion with no fragment target alone — an artifact, draft, or note target (per `../ai/CVL-conversation-loop.md` CVL-FR-08). No turn on a fragment-targeted discussion is attached it, and no graduation turn is (per `TLC-tool-conventions.md` TLC-FR-24), so the pending set exists in discussions and nowhere else.
 5. **ADQ-FR-PVXK** `questions` is required and holds 1 to 10 entries. Each entry's `question` is non-blank once trimmed, and its `options` holds 2 or 3 entries, each non-blank once trimmed. An empty list, more than ten questions, blank question text, an option count outside 2 to 3, and a blank option are each a retryable `InvalidArgs` refusal naming the question at fault.
 6. **ADQ-FR-CJRM** Nothing malformed is dropped and salvaged. One bad question or one blank option refuses the **whole** call and records nothing, so the author is never shown a set smaller than the agent meant to ask.
    - *Why:* A silently trimmed set costs a decision taken on less than was meant, where a refused call costs one round.
 7. **ADQ-FR-ZBQH** Every value is bounded by a named limit — each `question` 2 KiB and each option `value` 512 B — and a value above its limit is the retryable `InvalidArgs` **too long** refusal naming which limit and which question.
    - *Why:* The set is persisted and rendered, and the comments it generates travel to a later turn inside an input already bounded (per `../core/AGC-agent-conversations.md` AGC-FR-11).
 8. **ADQ-FR-TWNS** The application assigns each question and each option a **stable position** in the order the model wrote them, ascending from 1, and that position is what every later act names. The text is never an identifier, and the model neither supplies a position nor sees one.
    - *Why:* Two questions may read alike, so an answer matched by text would attach to whichever matched first.
 9. **ADQ-FR-GMDK** A valid call creates **one** `PendingQuestionSet` through `../core/CMS-comments-storage.md`'s `reserve_discussion_question_set` (CMS-FR-QLDW), carrying a fresh `set_id`, the discussion's thread id, the turn's agent participant, the instant it was recorded, and every question in the order given with its own options as given. Ten questions are one set and one decision rather than ten of either.
10. **ADQ-FR-XKVR** A discussion holds **at most one** pending set, and one call creates at most one. The discussion's pending-set slot is **reserved atomically before the set is created**, so two calls from two agent turns racing on one discussion leave exactly one set whichever arrived first.
11. **ADQ-FR-QNJU** A call made while the discussion already holds a pending set is the **already pending** refusal and records nothing. The standing set is not replaced, merged into, queued behind, or partially added to, and no comment is appended, so the author is never shown two sets about one discussion.
12. **ADQ-FR-RWTP** That refusal is **not retryable** and says so in its message (per TLC-FR-11), no argument the model could compose freeing a slot the author holds.
13. **ADQ-FR-BSYE** A refusal of any kind **does not end the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-14): the refusal is appended to the exchange like any other tool result and the loop continues, so an agent whose set was refused answers with what it already has. Only a call that actually recorded ends the turn (per CVL-FR-15).
14. **ADQ-FR-DHZK** A successful call **appends no comment**. The pending set is the whole of what the author sees until they submit, and the discussion's log is byte-for-byte what it was.
15. **ADQ-FR-FQPA** The turn ends on the successful call and terminates `awaiting_reply` (per `../ai/CVL-conversation-loop.md` CVL-FR-15, `../core/AGC-agent-conversations.md` AGC-FR-VRHM). Because no comment was appended, that state carries **no ordinary conversation contribution**, and the durable set is the only user-facing question state the turn leaves behind.
16. **ADQ-FR-PZWD** The set is persisted per discussion and **survives an application restart**. A restart restores the questions, their original order, each question's text, and each question's options, and restores **no** selected option, **no** own answer, and **no** note text. It restores no agent turn, no model exchange, and no presentation instance (per `../core/AGC-agent-conversations.md` AGC-FR-ZPKW, `../ui/CVP-conversation-presentation.md` CVP-FR-48).
17. **ADQ-FR-MRSK** An accepted submission appends, in the **recorded question order**, one agent-authored **question** comment and then immediately one human-authored **answer** comment per question, with the bodies of the contract surface. The question comment is stamped with the set's recorded participant snapshot (per `../core/CMS-comments-storage.md` CMS-FR-41) and the answer comment with the acting human identity (CMS-FR-11).
18. **ADQ-FR-RECR** An **answer** comment opens with `**Selected option:**` and the chosen option's recorded value where the author chose one, and with `**Own answer:**` and the author's words where the author wrote their own. A `**Note:**` paragraph follows only a chosen option, and only where the note is non-blank.
    - *Why:* No marker in the body is permitted (ADQ-FR-NUEB), so the opening line is the whole of what tells a reader, and the surface that pairs the comments, which kind of answer this is.
19. **ADQ-FR-NUEB** An option value, an author's own words, and a note are **escaped for Markdown without changing their text**, so a value carrying an asterisk or a backtick reads in the comment exactly as it was written. Neither body carries a routing tag, an identifier, a machine-readable field, or any marker that a tool composed it.
20. **ADQ-FR-YQTB** The `set_id` is the **idempotency identity** of the submission. Every comment event it appends carries an identity derived from that `set_id` and the question's position — `<set-id>:<position>-1q` for a question and `<set-id>:<position>-2a` for its answer, the position written as two digits with a leading zero below ten — so a retry derives the same identities and the fold ignores the duplicates (per `../core/CMS-comments-storage.md` CMS-FR-06). The derived identity **sorts into the recorded question order**: the position leads it so that ten questions order as the author was asked them rather than as text, and the rank digit puts each question ahead of its own answer. The fold replays events in `at` order and tie-breaks on this identity (per `../core/CMS-comments-storage.md` CMS-FR-09), and one submission commits at one instant, so this ordering is the whole of what makes the appended pairs read in the order ADQ-FR-MRSK states.
    - *Why:* The pairs are joined for the reader by adjacency alone (per `../ui/DQA-discussion-question-answering.md` DQA-FR-GRUV). An identity that sorted otherwise would separate every question from its answer, and no rendering could put them back together.
21. **ADQ-FR-LVOC** The pending set is deleted **only after** the ordered append is committed (per `../core/CMS-comments-storage.md` CMS-FR-TXRB). A submission that is refused, or whose append does not commit, leaves the set exactly as it stood, so the author retries without re-entering anything (per `../ui/DQA-discussion-question-answering.md` DQA-FR-WKTP).
22. **ADQ-FR-EJHR** A retry after a failure or a restart returns or completes the **same ordered result**. It never appends a subset, never duplicates a comment, never deletes the set before the append commits, and never loses the set.
23. **ADQ-FR-ZXAF** The answers reach the agents as **fresh later turns** and never by resuming the turn that asked, which ended before there was an answer. The asking turn is retired or discarded without being resumed (per `../core/AGC-agent-conversations.md` AGC-FR-TQLC), and the dispatch follows the discussion's ordinary routing once for the whole submission (per `../ui/CTA-comment-agent-turns.md` CTA-FR-PXJB).
24. **ADQ-FR-SCUW** The tool records and returns. It waits for no reply, holds no timer, opens no watcher, and leaves no request outstanding, so TLC-FR-16 holds for it unchanged.
25. **ADQ-FR-VDGT** A discussion whose folded state is **locked** (per `../core/CMS-comments-storage.md` CMS-FR-17) is the not-retryable `PermissionDenied` refusal and nothing is recorded, a conversation that takes no further contribution taking no further question either.
26. **ADQ-FR-HBLN** A reservation that fails for any other reason is the retryable `Other` refusal, and nothing was recorded: the discussion holds no set and its log is byte-for-byte what it was.
27. **ADQ-FR-KOWX** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), there being no discussion to record against.
28. **ADQ-FR-UIYD** The tool holds no state between calls and no state of its own beyond the turn its constructor bound it to (per TLC-FR-15). Two turns hold two instances and neither observes the other's call; what keeps them from both recording is the reservation of ADQ-FR-XKVR rather than anything either instance holds.
29. **ADQ-FR-RMBC** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record when a set is recorded, naming the tool, the discussion, and the question count, and a `WARN` record when it refuses, naming the tool, the discussion, and the reason.
30. **ADQ-FR-LZHV** **No record carries** a question's text, an option's value, an author's own words, a note, any part of a generated comment body, or any part of the conversation. The question count is the one thing this tool names as loggable beyond the discussion its constructor bound it to.
31. **ADQ-FR-JAQE** The tool reaches no network and touches no credential (per TLC-FR-18): recording a set is a local write, and nothing about a question leaves the machine.
32. **ADQ-FR-OFZM** The pending set is destroyed with the thing the discussion belongs to and by no other route: a deleted draft takes its discussions' sets with it (per `../core/CMS-comments-storage.md` CMS-FR-39) and a deleted note takes its own (CMS-FR-64). Nothing collects, expires, or times out a set the author has not answered.

## Non-functional requirements
- The set costs one small JSON write and one durable read, so a discussion carrying questions costs nothing beside the model call the turn had already made.
- Asking ten questions costs the author one visit rather than ten, which is the whole reason the tool exists: a discussion settled over ten round trips is ten turns of gathering against material that keeps moving.
- Nothing here requires network access, and nothing about a question or an answer leaves the machine except by the route the discussion's own log already takes.
- The description is written so a model reading every tool description in a discussion turn can tell this one from `AUC-ask-user-comment-tool.md`'s on its first sentence: this one asks several things with options, that one asks one thing openly.
- A set the author never answers holds nothing open: the call returned when it recorded, no turn is suspended, and no timer expires it. It costs one file until the discussion's owner goes.
- The tool is constructed per turn rather than once for the application, which costs a pair of values copied into a struct.
