<!--
  EAC-FR-35. The shape of an answer, stated in words, added to every task
  document for every vendor and every caller.

  It exists because one pinned vendor cannot be handed a schema
  (`../../../specifications/infra/CDX-codex-cli-protocol.md` CDX-FR-14) and no
  caller chooses its vendor, so a contract stated only where a schema can reach
  is a contract half the runs never receive.

  Two rules bind edits here. It must state the same structural rules
  `src-tauri/src/tools/agent_exec/protocol.rs` enforces (EAC-FR-18) and no
  weaker set — an agent that answers what this says must never be refused
  afterwards for having done so. And where this and that module's
  `ENVELOPE_SCHEMA` both describe a rule, the two must say the same thing
  (CCP-FR-07), so an agent reading both is told one contract rather than two.

  It says what an answer looks like and never what the work is. Which outcome a
  particular turn had is the caller's to state in its own instruction.
-->

Answer with exactly one JSON document. It is the whole of what the caller reads:
prose around it is not read, more than one document is not read, and a turn that
answers in prose alone has not answered.

The document carries these fields and no others. A field this list does not name
is refused, whatever it holds.

- `protocol_version` — the integer `1`. Required.
- `outcome` — one of `"success"`, `"failure"`, or `"escalation_required"`. Required.
- `summary` — one line saying what the turn did. Required, and never empty.
- `result` — an object carrying whatever the caller's own instruction asked you
  to report back. On `"success"` only.
- `failure` — an object of `code`, `message`, and `retryable`, describing work
  that could not be done. On `"failure"` only, and required there.
- `escalation` — an object of `reason` and `questions`, describing decisions that
  are not yours to take. On `"escalation_required"` only, and required there. Its
  own shape is set out below.
- `session` — the caller reads the session identity from the CLI itself, so
  there is nothing here for you to fill in.
- `metadata` — carries nothing. It accepts no key, and a key put there does not
  reach the caller.

Which fields may accompany which outcome is exact. A document that mixes them is
refused whole rather than read in part:

- `"success"` carries `result` where there is something to report, and carries
  neither `failure` nor `escalation`.
- `"failure"` carries `failure`, and carries neither `result` nor `escalation`.
- `"escalation_required"` carries `escalation`, and carries neither `result` nor
  `failure`.

The `escalation` object carries these fields and no others. A singular
`question` field, and an `options` field at this level, are refused as the
undefined fields they are:

- `reason` — why the turn cannot finish, in a sentence or two. Required, never
  empty, and at most 4096 bytes.
- `questions` — everything you are stuck on, as a list of between one and eight
  entries in the order they are to be answered. Required. The order is the only
  identity a question has: the caller numbers them itself and answers them by
  that number, so nothing is sorted, dropped, or added.

Each entry of `questions` carries:

- `question` — one thing asked, never empty, at most 2048 bytes. Required.
- `options` — up to three concrete responses to that question, where it has
  them. Leave it out where no fixed response suits; an empty list means the same
  thing.

Each entry of `options` carries all three of these:

- `answer` — the value submitted when that response is chosen. Never empty, at
  most 512 bytes.
- `summary` — a label for the response: one sentence, at most five words, at
  most 128 bytes.
- `description` — what choosing it means: at most two sentences and twelve words
  altogether, at most 256 bytes.

One malformed question or one malformed response refuses the whole document
rather than yielding the questions that happened to parse.

A turn that ends in one of the three outcomes has answered, whichever one it is.
Reporting `"failure"` or `"escalation_required"` is answering in the protocol
rather than failing to answer, and is read as the account of the turn that it is.
