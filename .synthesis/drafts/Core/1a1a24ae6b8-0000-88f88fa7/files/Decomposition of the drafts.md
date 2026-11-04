## Intent

Sometimes user might have a situation when the Draft he created grows to big and has to be decomposed into 2 or more separate drafts that will do the same job one by one sequentially.

## User journey

- User creates a draft
- User eventually sees that draft is too big or affects too many components. Or the conversation agent brings up this as a concern.
- User makes a decision to decompose the task, and asks an agent to decompose the current draft into two or more new drafts, superseeding the original draft. User might provide special quidance on how he sees decomposition.
- Agent asks clarifation questions using existing tooling until agent understands full picture of decomposition.
- Agent calls new tool `propose_new_draft` once per new draft, one call after another. Each call carries one name, one full body, the position N, and the total M that the agent expects at that time. The agent can use the user's decisions and comments on earlier drafts to shape the next draft. The agent opens the next proposal only after the user decided the open one. User can accept, reject, or discuss each proposal on its own, in the same way as for the draft changes proposal today. Each proposal card has a preview in a tab.
- Only the user ends the decomposition. When no proposal is open and at least one new draft is accepted, the discussion shows **Confirm decomposition done** below the newest decided proposal card. The agent cannot end the decomposition. When the user confirms, the original draft gets archived and shows a `Superseded` label. Before the confirmation, the original draft stays active and unchanged.
- User is able to proceed to work with new drafts.

## Requirements

- Look and feel must be aligned during this process. 
- New drafts must be created in the same folder as the original draft that’s being decomposed.
- The tool is bound to the draft under discussion. It is refused for a draft that is archived, graduated, held by a graduation run, or a GitHub-shadow draft. A new call is refused while a proposal of the same decomposition is open (not decided), and after the user confirmed the decomposition. The agent changes the open proposal with `change` and `withdraw` updates, as `propose_draft_changes` does.
- Each call makes one proposal record beside the original draft, posted as one comment of the agent. All proposals of one decomposition share one set id. Nothing is created until the user accepts a draft. Accepting a draft creates it in the original draft's folder, with the proposed name and body as its prompt. Rejecting creates nothing. Discussing opens a discussion about that draft and decides nothing. A name is not required to be unique.
- Each accepted draft stores its sequence: the set id, the position N, and the total M, as the agent gave them in its call. The stored values are not rewritten when a later call gives a different M. A rejected draft keeps its position, so the numbers do not change. The Drafts panel row of an accepted draft shows a `Part N of M` badge in the style of the `Published` badge. Its hover text and accessible name state the position and the name of the original draft. The panel keeps its current row order.
- The preview opens a read-only tab with the proposed body, rendered as a draft prompt. It creates no draft and no file. It closes with the proposal.
- `Superseded` follows the `Published` model (`DRP-FR-TQUB`): it is a fact stored in the original draft's metadata, never in its prompt, and its badge is the same style as `Archived`, `Graduated`, and `Published`. The confirmation sets the draft status to `archived` and stores the marker with the names of the accepted drafts. It never edits the original prompt and never deletes the draft. When every draft is rejected, no confirmation is offered and the original draft stays active.
- All states are keyboard operable and are not shown by colour alone.

## Expected deliverables

- New tool `propose_new_draft` with its tool specification, and its backend record, events, and agent tool registration.
- Draft storage for the proposal record, the sequence data of an accepted draft, and the `Superseded` marker.
- Review card for one proposed draft with Accept, Reject, Discuss, and a preview tab. **Confirm decomposition done** control in the discussion, shown when no proposal is open and at least one draft is accepted.
- Drafts panel badges `Part N of M` and `Superseded`.
- Updates to these specifications: `specifications/core/DRS-draft-storage.md`, `specifications/ui/DRP-drafts-panel.md`, `specifications/ui/NAW-new-artifact.md`, `specifications/core/DCP-draft-change-proposals.md`, `specifications/ui/DCR-draft-change-review.md`, and `specifications/tools/PDC-propose-draft-changes-tool.md` (for the shared update rules).