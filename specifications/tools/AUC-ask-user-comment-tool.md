# Ask user comment tool

**Spec code:** `AUC`

## Intent
The tool an agent reaches for when it cannot answer well without knowing something only the author can tell it. It posts **one question** into the conversation the agent was asked in — as an ordinary comment, in the agent's own name, offering a couple of candidate answers where it has them — and ends the agent's turn there. The author answers in the next comment, in prose, picking one of the candidates or saying something else entirely, and that reply brings the agent back with the whole conversation in front of it wherever the author is still addressing it — a comment reaches the agents it names, or the ones the conversation is already addressed to, and the question itself claims nothing further (per `../ui/CTA-comment-agent-turns.md` CTA-FR-QUXJ, CTA-FR-JQDM). The shape is deliberately the shape a person's clarifying question takes: one thing at a time, written as prose rather than as a form, posted and then quiet — an agent that has asked something stops talking and waits, exactly as a colleague who asked would, rather than holding the conversation open or answering every reading of an ambiguous request in turn. Nothing about the resulting comment tells a reader that a tool composed it, and nothing about answering it differs from answering a person: there is no control to click, no identifier to quote back, and no form of words the reply has to take. This is the one tool in the group whose effect a person sees, and it is the reason `TLC-tool-conventions.md` distinguishes a tool's declared side effects from the read-only posture the rest of the group shares. Out of scope: asking more than one thing, which this tool does not do — a discussion turn that can propose the likely answers puts its whole set through `ADQ-ask-discussion-questions-tool.md` instead, and this tool stays the one for a single open question anywhere; choosing where a question goes, since the tool posts into the conversation of the turn that holds it and can reach no other; waiting for the answer, since the call returns as soon as the comment is appended and holds no timer, watcher, or open request; resuming the turn that asked, which does not resume — the author's reply dispatches a fresh turn, and how a reply reaches the agent that asked is `../ui/CMT-comments.md`'s and `../core/AGC-agent-conversations.md`'s; and rendering the question, which is an ordinary comment the rail already draws.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`ask_user_comment` — a `rig` portable tool. Unlike the read-only tools of this group it is **constructed per turn** (per TLC-FR-15), its constructor taking the origin of the conversation the turn belongs to and the agent participant that turn answers as, so the conversation it can post into is fixed before the model composes a single argument.

### The description
The fixed text the model reads (per TLC-FR-05):

> Ask the author one clarifying question, when you cannot answer well without knowing something only they can tell you — which of two readings they meant, or which of several directions they want. Reach for it instead of guessing, and instead of answering every possibility in turn. Write the question as you would write it to a colleague, and pass a couple of candidate answers if you have them; the author may agree with one or say something else entirely. Your question is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's reply in front of you. Ask one thing at a time: because your turn ends here, anything else you asked for in the same message is not carried out. It posts into the conversation you were asked in and nowhere else, and it changes nothing in the project.

### Arguments
```
AskUserCommentArgs {
  question: string,      // required
  options:  string[]?    // optional; a few candidate answers, default none
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `question` — *"What you want to know, written as you would write it to a colleague in a comment. Ask one thing; if several are unclear, ask the one whose answer changes the most."*
- `options` — *"A few candidate answers, if you have them — two or three is usual. Each is a short phrase the author can agree with. Optional: omit it to ask an open question. The author is free to answer something else whichever you pass."*

### Output
Data (per TLC-FR-08): `{ posted: true }`. The turn ends on the call that posted (per `../ai/CVL-conversation-loop.md` CVL-FR-15), so no further model call reads this value — it is what the exchange records rather than anything the model acts on.

### The comment
What the author reads. The question as the model wrote it; then, where options were passed, a Markdown list of them, one to a line; then a single fixed closing line. Nothing else:

```
Should graduation write one spec or two?

- one — a single spec covering both layers
- two — paired ui/ and core/ specs

Or say what you'd rather do.
```

A question carrying no options is the question alone and carries no closing line, an open question needing no invitation to answer it openly.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank question** — kind `InvalidArgs`, retryable: *"The question must say what you want to know. Write it as you would write it to a colleague."*
- **conversation locked** — kind `PermissionDenied`, not retryable: *"This conversation is locked and takes no further comments, so nothing was posted and no answer is coming. Answer with what you already have; asking again will not succeed."*
- **question set pending** — kind `PermissionDenied`, not retryable: *"This discussion already has a question set waiting on the author, so nothing was posted. Answer with what you already have; asking again will not succeed."*
- **could not post** — kind `Other`, retryable: *"The question could not be posted to this conversation."*

### Side effects
Declared in full (per TLC-FR-17). One successful call appends **exactly one comment** to the conversation the turn belongs to, which is one line appended to the log that conversation already lives in — the same and only mutation a person posting a comment performs (per `../core/CMS-comments-storage.md` CMS-FR-04). Nothing else changes anywhere: no file of the project is created, modified, or deleted, no other store is mutated, and no repository state beyond that one line is touched. A refused call appends nothing.

## Functional requirements
 1. **AUC-FR-01** The tool exists as a `rig` portable tool named `ask_user_comment` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, and logging are that spec's rules applied to this tool. It is the tool of TLC-FR-17 that declares a side effect rather than a read-only posture, and its side effect is the one declared in the contract surface and nothing beyond it.
 2. **AUC-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05). Neither varies by conversation, by agent, or by project, so two agents holding this tool were offered byte-identical definitions of it however different the conversations they are in.
 3. **AUC-FR-03** The tool posts into the conversation of the turn that holds it and accepts no argument naming a conversation, a thread, a file, or a participant. Its constructor binds it to that turn's origin and agent participant (per TLC-FR-15), so no argument a model composes can reach a conversation it was not asked in, and an instance outlives no turn.
 4. **AUC-FR-04** The comment is appended through `../core/CMS-comments-storage.md`'s agent write path (CMS-FR-41) with the turn's agent participant — its agent id, its nickname as the handle, the model it runs as, and its title as the registry holds it at the moment of the append (per `../core/CMS-comments-storage.md` CMS-FR-65) — which is the same path and the same participant shape a delivered answer is appended through (per `../core/AGC-agent-conversations.md` AGC-FR-16). The fold, the rail, and the Comments panel therefore read a question exactly as they read any other comment.
 5. **AUC-FR-05** The comment's body is prose throughout: the `question` as the model wrote it less its citation markers (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU), then the `options` as a Markdown list one to a line where any were passed, then the single fixed closing line of the contract surface. It carries no identifier, no marker, no machine-readable field, no control, and no instruction to the author about the form a reply must take. A reader cannot tell from the comment that a tool composed it, and answering it is answering a comment.
 6. **AUC-FR-06** A `question` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface and nothing is appended.
 7. **AUC-FR-07** `options` is optional and forgiving (per TLC-FR-07): absent, empty, or holding nothing but blank entries, it is no options at all and the body is the question alone. Blank entries are dropped and the rest carried in the order given. A long list is not refused and not truncated — a model that offers six candidates is offering six, and the author reads them.
 8. **AUC-FR-08** The tool posts and returns. It waits for no reply, holds no timer, opens no watcher, and leaves no request outstanding, so TLC-FR-16 holds for it unchanged: the answer reaches the agent as a later turn rather than as this call's result.
 9. **AUC-FR-09** A successful call returns the data object of the contract surface. It composes no sentence about what it did (per TLC-FR-08), the fact of the posting being the whole of the result.
10. **AUC-FR-10** A conversation whose folded state is locked (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **not retryable** `PermissionDenied` refusal, and its message says a further call will not succeed (per TLC-FR-11). No argument the model could compose would unlock a conversation, and an agent told otherwise would spend its remaining calls asking a thread that cannot answer.
11. **AUC-FR-11** An append that fails for any other reason is the retryable `Other` refusal, and nothing was appended: a refused call leaves the conversation byte-for-byte as it was.
12. **AUC-FR-12** A refusal is not a turn's failure (per `../ai/CVL-conversation-loop.md` CVL-FR-14) **and does not end the turn**: a call that posted nothing leaves the loop running, so an agent whose question was refused answers with what it already has rather than falling silent. Only a call that actually posted ends the turn (per CVL-FR-15).
13. **AUC-FR-13** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), there being no conversation to post into.
14. **AUC-FR-14** The appended comment carries no attachment, on the same terms a delivered answer carries none (per `../core/AGC-agent-conversations.md` AGC-FR-17): an agent contributes prose to a conversation and never a file, whether it is answering or asking.
15. **AUC-FR-15** The comment this tool appends dispatches no turn, being agent-authored (per `../ui/CTA-comment-agent-turns.md` CTA-FR-YWSU). A question therefore never sets another agent working, and every call any agent makes is one the author asked for.
16. **AUC-FR-16** The tool holds no state between calls and no state of its own beyond the turn its constructor bound it to (per TLC-FR-15). Two turns hold two instances and neither observes the other's call, so two agents asked in one conversation each post their own question and neither's is attributed to the other.
17. **AUC-FR-17** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record when a question is posted, naming the tool and the conversation it was posted into, and a `WARN` record when it refuses, naming the tool, that conversation, and the reason. The conversation is named because a record that cannot be tied to one cannot be followed, and it is not an argument — it was bound at construction rather than composed by the model. **No record carries the `question`, any entry of `options`, any part of the comment's body, or any part of the conversation**, all of which are material a model composed out of a conversation it is having (per TLC-FR-14), so this tool names no argument as loggable.
18. **AUC-FR-18** The tool reaches no network and touches no credential (per TLC-FR-18): posting a comment is a local append, and nothing about a question leaves the machine except by the ordinary route the conversation's own log takes.
19. **AUC-FR-QSVN** A call made while the conversation is a discussion holding a **pending question set** (per `ADQ-ask-discussion-questions-tool.md` ADQ-FR-GMDK) is the **question set pending** refusal of the contract surface. It appends no comment, does not end the calling turn (AUC-FR-12), and leaves the standing set and everything the author has entered against it exactly as they were.
    - *Why:* A conversation already holding a set the author is answering would otherwise gain a second question they cannot reply to while the composer is disabled.

## Non-functional requirements
- The question is the only text in this group that a person reads, which inverts where the care goes: everywhere else in the group the expensive text is the description a model reads, and here it is the body a colleague reads. The tool composes as little of it as it can — the model's own words, a list, and one closing line — because every byte the tool adds is a byte that reads as machinery in a conversation that should read as prose.
- A call costs one line of I/O through the conversation's own append path, the same cost a person's comment costs.
- Nothing in this tool requires network access.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences: this one asks the author something, `RFT-read-file-tool.md`'s reads a file it is given the path of, `SPS-specification-search-tool.md`'s finds which specifications bear on a topic, and the three skill tools answer about skills.
- A question the author never answers costs nothing and holds nothing open: the call returned when it posted, and what became of the turn is `../core/AGC-agent-conversations.md`'s affair rather than this tool's.
- The tool is constructed per turn rather than once for the application, which is the one place this group pays for an instance. A construction is a pair of values copied into a struct, so the cost is nothing beside the model call the turn is about to make.
