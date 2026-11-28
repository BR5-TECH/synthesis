# Product vocabulary

**Spec code:** `PVC`

## Intent
The application uses the vocabulary of OpenSpec. The author works on a **change** in a conversation, and then starts its **implementation**. This spec gives the one word that every user-visible text and every agent-facing text uses for each concept, and the names of the agent tools. It also keeps the internal names that code, storage, and the specifications use, so that stored data and the corpus citations stay valid.

## Functional requirements
1. **PVC-FR-KTWB** The vertical panel of `../ui/CHG-changes.md` is the **Commits** panel in every user-visible text: its activity-bar toggle, its header, and each action that opens it, which reads **Open Commits**.
2. **PVC-FR-HMJD** A draft of `DRS-draft-storage.md` is a **change** in every user-visible text, and two or more are **changes**. The panel of `../ui/DRP-drafts-panel.md` is the **Changes** panel.
3. **PVC-FR-ZQRE** A graduation of `GRD-graduation.md` is an **implementation** in every user-visible text. The action is **Implement**, the terminal status is **Implemented**, the status of a change that a run holds is **Implementing**, and a graduation run is an **implementation run**.
4. **PVC-FR-QWAN** The section of the Runs bottom panel of `../ui/GRU-graduation-runs.md` and the project settings section of `../ui/SET-project-settings.md` that hold graduation runs and their limits are named **Implementation**.
5. **PVC-FR-NEXL** The folder that a propose run writes in `openspec/changes/<name>/` is the **OpenSpec change** in every text that can name a change of PVC-FR-HMJD too. Such a text never calls it a bare "change".
6. **PVC-FR-VBPY** One proposed hunk of a proposal to the prompt of a change is an **edit**, in agent-facing text and in the user-visible text of that proposal.
7. **PVC-FR-CUAT** Every agent-facing text uses the words of PVC-FR-HMJD, PVC-FR-ZQRE, PVC-FR-NEXL, and PVC-FR-VBPY. Agent-facing text is every prompt under `resources/prompts/`, every built-in skill under `resources/skills/`, the text that the backend writes into the input of a turn, and every tool description, parameter description, and refusal message.
8. **PVC-FR-RWDM** The agent tools carry exactly the names of the **Tool names** table of the contract surface. A tool that acts on an OpenSpec change has `openspec` in its name. A tool that acts on a change of PVC-FR-HMJD has `change` in its name and no `openspec`.
9. **PVC-FR-LGYT** A stored tool call that carries a **legacy name** of the **Legacy names** table is read as a call of the tool's current name, where the stored call is read again: in the history that a turn sends to a model, in the exchange replay of `../ai/ERP-exchange-replay.md`, and in the activity of `../ui/CTA-comment-agent-turns.md`.
10. **PVC-FR-PXOE** A legacy name maps to a current name only for a conversation whose target kind the **Legacy names** table gives for that name. A stored call with any other name, or on another target kind, keeps its name.
    - *Why:* three legacy OpenSpec names are now the current names of the change tools, so the target kind alone tells which tool an old call used.
11. **PVC-FR-SQAF** `read_change` and `measure_document` accept `draft_id` as an alias of `change_id`. `propose_plan_changes` accepts the `op` value `new_draft` as an alias of `new_change`. The schema that a model receives names `change_id` and `new_change` alone.
12. **PVC-FR-JBPC** The activity status of a call with a legacy name is the status of its current name (per `../ui/CTA-comment-agent-turns.md` CTA-FR-HEYB).
13. **PVC-FR-TKJW** A field of a tool output that names a change of PVC-FR-HMJD has `change` in its name. `read_change` and `search_changes` return the id of a change as `change_id`, and `search_changes` returns its matches in a `changes` list.
14. **PVC-FR-MBQE** A storage commit that the application makes for a change has the subject `change: create "<name>"`, `change: delete "<name>"`, or `change: implement "<name>"` (per `PST-project-storage.md` PST-FR-YWXF). A commit that an older build made keeps its subject.
15. **PVC-FR-ODXM** The internal names do not change. These are code identifiers, Tauri commands, events, storage paths and file names, settings keys, stored values (the draft status `graduated`, the plan row kind `new_draft`), refusal codes, conversation target kinds, material tags, task-input fields, spec codes, and spec file names.
    - *Why:* a rename of an internal name breaks stored projects and thousands of citations, and the author never sees one.
16. **PVC-FR-HGTI** A specification uses "draft" as the internal name of a change of PVC-FR-HMJD and "graduation" as the internal name of an implementation of PVC-FR-ZQRE. A user-visible label that a specification quotes uses the words of this spec.
17. **PVC-FR-UFTN** A GitHub draft pull request, and the GitHub review state **Changes requested**, keep their GitHub names. They are not changes of PVC-FR-HMJD.

## Contract surface
This spec registers no command and no event. It holds the tables that the requirements above use.

**Vocabulary**

```
concept (internal name)                    user-visible and agent-facing word
-----------------------------------------  ----------------------------------
Changes panel (CHG)                        Commits panel
draft (DRS)                                change
Drafts panel (DRP)                         Changes panel
graduate / graduated / graduating          Implement / Implemented / Implementing
graduation run                             implementation run
Graduation (Runs section, settings)        Implementation
draft template                             change template
OpenSpec change (OSM)                      OpenSpec change
proposed hunk of a draft proposal          edit
```

**Tool names**

```
tool                               spec
---------------------------------  -----------------------------------------------
search_changes                     ../tools/DST-draft-search-tool.md
read_change                        ../tools/RDT-read-draft-tool.md
propose_change_edits               ../tools/PDC-propose-draft-changes-tool.md
decide_change_edits                ../tools/DDC-decide-draft-changes-tool.md
propose_new_change                 ../tools/PND-propose-new-draft-tool.md
read_implementation_file           ../tools/RGF-read-graduation-file-tool.md
propose_openspec_change_edits      ../tools/PCE-propose-change-edits-tool.md
decide_openspec_change_edits       ../tools/DCE-decide-change-edits-tool.md
propose_new_openspec_change        ../tools/PNC-propose-new-change-tool.md
request_openspec_change_action     ../tools/RCA-request-change-action-tool.md
```

**Legacy names**

```
legacy name               current name                      target kind
------------------------  --------------------------------  ------------------
search_drafts             search_changes                    every kind
read_draft                read_change                       every kind
propose_draft_changes     propose_change_edits              every kind
decide_draft_changes      decide_change_edits               every kind
propose_new_draft         propose_new_change                every kind
propose_change_edits      propose_openspec_change_edits     change
decide_change_edits       decide_openspec_change_edits      change
propose_new_change        propose_new_openspec_change       change
request_change_action     request_openspec_change_action    change
```

A conversation on the target kind `change` (an OpenSpec change, per `../ai/CVL-conversation-loop.md` CVL-FR-EZKH) is never attached the change tools of PVC-FR-HMJD, so the last four rows cannot match a current call. An implementation turn replays no stored tool call, so `read_graduation_file` has no row.

## Non-functional requirements
- The rename is a change of words alone. No behaviour, no storage layout, and no command changes.
- A project that an older build of the application wrote opens and works without a migration.
