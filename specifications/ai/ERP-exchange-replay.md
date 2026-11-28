# Exchange replay

**Spec code:** `ERP`

## Intent

An agent in a conversation often needs, in a later turn, the material it already read in an earlier turn. Replay gives a turn the agent's own earlier exchanges — its replies, its tool calls, and their results — as real messages, so the agent does not read and search again what it already holds. The replayed messages are a stable prefix of the request, so the provider cache of `CVL-conversation-loop.md` CVL-FR-23 serves them on the next turn. The replay is kept in memory for the running application alone: a relaunch costs one rediscovery, and nothing new is written to disk.

## Functional requirements

1. **ERP-FR-ZVJT** Each open project has one **replay memory**. It holds the **replay entries** of its discussions, keyed by the discussion id and the agent id. It exists in memory alone. It is not written to disk, to a log record, to an event payload, or to a statistics log. Closing the project or the application drops it.
2. **ERP-FR-SQZM** A replay entry holds: the id of the trigger comment of its turn; the id of the comment that holds its answer, or null; the provider and the model id of its turn; the names of the tools the turn was offered; its **rounds**; its **stored state**; and its source stamps (ERP-FR-NKLF).
3. **ERP-FR-GWPG** The rounds of an entry are the messages that its turn appended to its exchange after its opening message, in their order. They leave out the line of `CVL-conversation-loop.md` CVL-FR-FSAY and the final reply that holds the answer.
4. **ERP-FR-UFBV** The stored state of an entry is the `discussion_state` section of its turn's input (per `../core/AGC-agent-conversations.md` AGC-FR-06), where it is 4,000 characters or fewer. A larger state is stored as one line: `[The discussion state of this turn is omitted.]`.
5. **ERP-FR-FEAD** A turn is **captured** into one entry when it terminates `delivered` with an answer comment, or terminates `awaiting_reply` (per `../core/AGC-agent-conversations.md` AGC-FR-28). A turn that ends on a question, a question set, a proposal, or a plan card has a null answer comment, and its comment stays in `discussion_history`.
6. **ERP-FR-QZNV** The reply that ends a turn on a contribution (per `CVL-conversation-loop.md` CVL-FR-15) stays in the rounds with the result of the call that ended the turn. Each other call of that reply gets the result `Not run: the turn ended on <tool>.`, where `<tool>` is the name of the ending tool.
7. **ERP-FR-CRJL** These turns are not captured: a turn that fails or is cancelled; an advisor turn; a synthesis turn; a turn that a speech registered; and a turn that took a steering message, held a placeholder, compacted, or recovered from a context overflow (per `CVL-conversation-loop.md` CVL-FR-EZKC, CVL-FR-KPOY, CVL-FR-EBJG, CVL-FR-QJLU).
   - *Why:* A later request cannot rebuild these exchanges byte for byte, or the exchange speaks for more than one agent.
8. **ERP-FR-CMMA** A turn **selects** the entries of its own discussion and agent alone. An entry is selected where its trigger comment, and its answer comment where it has one, are in the discussion before the trigger comment of the turn. The selected entries are in the order of their trigger comments.
9. **ERP-FR-BXWA** The entries of another agent are never replayed to a turn. Their comments reach the turn as text in `discussion_history`, as every comment does (per `../core/AGC-agent-conversations.md` AGC-FR-06).
10. **ERP-FR-DHKJ** An advisor turn selects no entry. A Meta turn selects its own entries, with the calls of `ask_advisors` and their results in its rounds.
11. **ERP-FR-OPRO** With entries selected, the exchange of a turn opens with, for each entry in order: one user message, the **opening** of the entry; its rounds; and, where its answer comment is not null, one assistant message with the answer. Then follow the user message of the current input and the rounds of the current turn.
12. **ERP-FR-ISTB** The opening of E1 holds the material sections of the current dispatch (per `../core/AGC-agent-conversations.md` AGC-FR-13), `discussion_history` with the comments before the trigger of E1, `current_comment` with that trigger, and the stored state of E1.
13. **ERP-FR-VMRY** The opening of each later entry holds `discussion_history` with the comments after the trigger of the entry before it and before its own trigger, `current_comment` with its trigger, and its stored state. It holds no material section.
14. **ERP-FR-LKUP** With entries selected, the user message of the current input holds `discussion_history` with the comments after the trigger of the last selected entry, `current_comment`, the current `discussion_state`, and the other sections of the turn (per `../core/AGC-agent-conversations.md` AGC-FR-HVQD, AGC-FR-DIZM). It holds no material section.
15. **ERP-FR-FMMT** A comment that an entry replays as its answer is in no `discussion_history` section of the exchange.
16. **ERP-FR-BBXA** The answer of an entry is the body of its answer comment as stored now, with its citation markers removed (per `CVL-conversation-loop.md` CVL-FR-VSSU). An entry whose trigger comment or answer comment is no longer in the discussion is dropped from the memory and is not selected.
17. **ERP-FR-IVGL** Every opening is rendered by the renderer of the input (per `CVL-conversation-loop.md` CVL-FR-37). While no comment, stamp, budget, or tool set changes, the last request of a turn is a byte-for-byte prefix of the first request of the next turn of the same agent in the same discussion.
    - *Why:* That prefix is what a provider cache holds from one turn to the next (per `CVL-conversation-loop.md` CVL-FR-23).
18. **ERP-FR-SBBE** A turn with no selected entry opens its exchange with its input as one message (per `CVL-conversation-loop.md` CVL-FR-12).
19. **ERP-FR-UGII** A round with a call of a tool that the current turn is not offered is replayed as **text**. Its reply becomes one assistant message with the text of the reply and one line per call that names the tool and its arguments. Its results become one user message with the text of each result.
20. **ERP-FR-LXFF** Where the provider and the model of the current turn are those of the entry, its rounds are replayed as stored, with the thinking and the provider's identifiers that `CVL-conversation-loop.md` CVL-FR-ETAH keeps.
21. **ERP-FR-XJGI** Where the provider or the model differs, the rounds lose their reasoning and every provider-native item. Each call and its result get a new id of 9 base-36 characters, derived from the entry and the position of the call. The same entry gives the same ids on every turn.
21. **ERP-FR-GAXX** Where the provider of the turn is `custom`, or a call id of an entry is also the id of a call of another selected entry, each call of the entry and its result get the derived id of ERP-FR-XJGI. Their content stays as stored.
    - *Why:* An OpenAI-compatible server can give ids by position, such as `call_0`, and the same id twice in one exchange is ambiguous.
22. **ERP-FR-PBCM** No replayed message holds a provider-native search or fetch transcript (per `CVL-conversation-loop.md` CVL-FR-31) or a provider citation marker (per `CVL-conversation-loop.md` CVL-FR-VSSU).
23. **ERP-FR-NKLF** A result of `read_file`, `read_change`, `read_openspec_spec`, or `measure_document` whose call names a file that resolves records a **source stamp**: the call id, the root, the path, and the SHA-256 of the bytes of the file, read through `../core/FSA-filesystem-access.md`. Other results record no stamp.
24. **ERP-FR-RVYX** Before a turn builds its exchange, it reads again at most 200 stamps of its selected entries. A result whose file changed or is gone becomes `[Outdated: <path> changed after this result was read. Read it again if you need it.]`. The new text is written into the entry, and the stamp is removed.
25. **ERP-FR-FVVU** A result with no stamp is replayed as it was made. The prompt tells the agent that an earlier result shows the state at the time it was made (per `CVL-conversation-loop.md` CVL-FR-GWTI).
26. **ERP-FR-UUAD** A turn has a **replay room** of `min(15% of window, 60% of window − E)` tokens, where `E` is the estimate of its request with no entry selected (per `CVL-conversation-loop.md` CVL-FR-PGII). The rounds and the stored states of the selected entries count against the room. The openings and the answers count against the bound of `../core/AGC-agent-conversations.md` AGC-FR-HOND.
27. **ERP-FR-GVHU** Where the rounds and states are above the room, the turn replaces the results of the oldest entries first with `[Result omitted to save room. Read it again if you need it.]` until they fit three quarters of the room. The new text is written into the entry.
    - *Why:* A cut to three quarters keeps the prefix the same for several turns, and a cut to the room itself changes it on every turn.
28. **ERP-FR-GBOK** Where the rounds and states are still above three quarters of the room with every result replaced, the turn **demotes** the oldest entries first until they fit. A demoted entry is removed from the memory, and its answer comment stands as text in `discussion_history`.
29. **ERP-FR-IICX** Where `discussion_history` must drop comments (per `../core/AGC-agent-conversations.md` AGC-FR-XOZQ), it drops them until the rest fits three quarters of its bound. The memory keeps the first comment kept for the discussion and agent. A later turn starts its history at that comment while the rest fits the bound.
29. **ERP-FR-QJEM** Where the anchor, or the newest comment alone, does not fit the bound of `../core/AGC-agent-conversations.md` AGC-FR-HOND, the turn keeps the history section that its builder made and selects no entry.
30. **ERP-FR-NCEH** A selected entry is marked as used by its turn. Each discussion and agent holds at most 20 entries, and a capture beyond that drops the oldest. Each memory holds at most 32,000,000 characters of entries, and a capture beyond that drops the entries used least recently first.
31. **ERP-FR-DSAP** The older history of a turn (per `CVL-conversation-loop.md` CVL-FR-XCOY) includes every replayed message and the openings less their material sections. A compaction gives a request with no replay, whose input holds the material sections first again (per `CVL-conversation-loop.md` CVL-FR-BSMY).
32. **ERP-FR-WZAV** No result bound cuts a replayed result again (per `CVL-conversation-loop.md` CVL-FR-XMGI). A recovery from a context overflow cuts the results of the latest round of the current turn alone (per `CVL-conversation-loop.md` CVL-FR-QJLU).
33. **ERP-FR-GDLV** `estimate_discussion_context` selects the entries of its agent and discussion on the same terms as a turn and counts them (per `../core/AGC-agent-conversations.md` AGC-FR-JFBZ). It changes no entry.
34. **ERP-FR-XTQI** The `split` counts the openings and the answers as `history`, the replayed rounds and stored states as `replayed`, and the results of the current turn alone as `tool_results` (per `CVL-conversation-loop.md` CVL-FR-AAPA).
35. **ERP-FR-GOXP** A turn with selected entries emits one `DEBUG` record with the correlation fields of `CVL-conversation-loop.md` CVL-FR-28. It names the number of entries selected, demoted, and with replaced results, the number of outdated results, and the estimated tokens of the replay. It holds no part of the exchange.
36. **ERP-FR-KIZX** The tool list saved on a comment (per `../core/AGC-agent-conversations.md` AGC-FR-WNQE) is not a source of replay. A replay reads the replay memory alone.

## Contract surface

### Internal (no UI consumer; not Tauri commands and unreachable from the frontend)

```
ReplayKey   { discussion_id, agent_id }          // per open project (ERP-FR-ZVJT)

ReplayEntry {
  trigger_comment_id,
  answer_comment_id: id | null,                  // null for a turn that ended on a contribution
  provider, model_id,
  tool_names: string[],                          // the tools the turn was offered
  rounds: Message[],                             // ERP-FR-GWPG
  state: InputSection[],                         // ERP-FR-UFBV
  stamps: SourceStamp[]                          // ERP-FR-NKLF
}

SourceStamp { call_id, root, path, sha256 }

ReplayBlock {
  opening: InputSection[],                       // ERP-FR-ISTB, ERP-FR-VMRY
  rounds:  Message[],
  answer:  string | null
}
```

- `capture_exchange(turn)` — stores one entry for a turn that terminates on the terms of ERP-FR-FEAD, and does nothing for one of ERP-FR-CRJL.
- `select_replay(origin, agent, trigger_comment_id, tools, endpoint, budget)` — the `ReplayBlock[]` of a turn, after the stamp check (ERP-FR-RVYX) and the room (ERP-FR-UUAD). It is placed in `AgentRequest.replay` (per `CVL-conversation-loop.md`).

## Non-functional requirements

- The replay memory is the largest in-memory store of a conversation. Its caps bound it to tens of megabytes per project.
- A relaunch, a project close, and an eviction cost the agent one rediscovery, and never a wrong answer.
- A change to a section renderer changes the prefix of every replayed turn once and costs one cache write. It changes no answer.
