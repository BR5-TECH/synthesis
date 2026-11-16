# Map proposal review

**Spec code:** `MPV`

## Intent
The card on which the author decides the work that an agent proposed for the delivery plan in a Map discussion. One agent comment carries one card. The card has one row for each proposed new issue, issue change, or new draft, and the author accepts or rejects each row on its own. Nothing reaches GitHub, and no draft is created, before the author accepts the row that does it. The card follows the pattern of `DCR-draft-change-review.md`: a decision is per item, a refusal shows beside the item it names, and every control works by keyboard alone. The card renders in the Discussion column of the Map tab (`SMP-specification-map.md`). Out of scope: the proposal and the application of a row, which are `../core/MPR-map-plan-proposals.md`; and the tool that records a proposal, which is `../tools/PPT-propose-plan-changes-tool.md`.

## Functional requirements
1. **MPV-FR-IBSF** A comment that carries a `map_plan_proposal` attachment renders one card below the comment body. The card is read with `"list map plan proposals (discussion id)"` and has one row for each row of the proposal, in proposal order. The comment body is the agent's rationale, and the card states no rationale of its own.
2. **MPV-FR-OCYF** A row shows its kind (`New issue`, `Change issue`, or `New draft`), its title or name, and for an issue its number. A new issue shows its body in a collapsed block, its Type, its milestone, its parent, and its labels. A new draft shows its name and its prompt in a collapsed block.
3. **MPV-FR-CCDM** A change row shows each changed field as the value before and the value after, side by side. A text field shows the text before struck through and the text after. A label change shows each added label with a plus sign and each removed label with a minus sign. A Status change shows both names.
4. **MPV-FR-LYKS** A close change shows the word `Close` and its reason in words (`completed` or `not planned`) in the row, before the author can accept. The reason is part of the accessible name of the **Accept** control.
5. **MPV-FR-XYDR** Each row has its own **Accept** and **Reject**. **Accept** invokes `"accept map plan row (discussion id, proposal id, row key)"` for that row alone. **Reject** invokes `"reject map plan row (discussion id, proposal id, row key)"` for that row alone. A decision changes no other row.
6. **MPV-FR-FTXV** A row states its state in words: `Waiting for you`, `Accepted`, `Rejected`, or `Failed`. The state is not shown by colour alone. An accepted row shows the link or the number of the issue it created, or a link that opens the new draft. A decided row has no **Accept** and no **Reject**.
7. **MPV-FR-WQKV** A row whose write failed shows the state `Failed` and its typed error text beside it. A `changed_on_tracker` error says that the issue changed on GitHub after the proposal and that nothing was written. A failed row offers neither **Accept** nor **Reject**, and it changes no other row.
8. **MPV-FR-FUQG** A row whose parent is a new issue of the same card shows `Waiting for its parent` with the title of the parent, while that parent is `Waiting for you`. Its **Accept** is disabled with that reason as its accessible description. When the parent is accepted, the row is accepted as any row is.
9. **MPV-FR-YEIH** A row whose parent row is rejected or failed shows that reason in words, for example `Its parent was rejected`, and its **Accept** stays disabled. Its **Reject** stays available while it is `Waiting for you`.
10. **MPV-FR-QOPR** While a decision runs, the row shows a busy state and its two controls are disabled. The other rows stay operable.
11. **MPV-FR-XAOG** Every row, its disclosure of the body, **Accept**, and **Reject** are reachable and operable by keyboard alone, in the order of the rows. A row is a group with an accessible name made of its kind, its title, and its state. A change of state is announced to assistive technology.
12. **MPV-FR-CZDO** The card follows `"map plan proposals changed"` for its discussion and re-reads the proposal. A decision made in another window, or by a refresh of the plan, shows without a reload.
13. **MPV-FR-GYLV** The card header states the count of rows and the count that are still `Waiting for you`. When no row waits, the header says that all rows are decided.
14. **MPV-FR-FESO** The composer of the Map discussion stays usable while a card has a row that waits. The card tells the author that the agent proposes again after every row is decided.
15. **MPV-FR-WKVL** An accepted row that wrote to GitHub or created a draft shows an Info toast, and a failed decision shows an Error toast (per `NTF-notifications.md` NTF-FR-TSRL). The row keeps its own state and error.
16. **MPV-FR-XYXY** A card whose rows are all decided stays in the thread as the record of what was decided. It stays readable and offers no control.
17. **MPV-FR-NMYT** A change row names the item it changes as a chip that selects that item on the Delivery lens canvas (per `MDL-map-delivery-lens.md`). Where the item is not on the canvas, the chip is plain text.
18. **MPV-FR-KMJX** The card logs a decision, its outcome, and its typed error through the frontend logging helper. No record carries a title, a body, a label name, or a draft name.

## Wireframes
```
┌ agent comment ──────────────────────────────────────────────────────────┐
│ The HLD names three parts the plan lacks. I propose one step: the first │
│ two issues and a draft.                                                 │
│ ┌ Plan proposal · 3 rows · 3 waiting ──────────────────────────────────┐│
│ │ New issue  Payout schedule                    Waiting for you        ││
│ │   Type Feature · Milestone GA · Labels payments  ▸ Body              ││
│ │                                          [ Accept ] [ Reject ]       ││
│ │ New issue  Payout retries  parent: Payout schedule                   ││
│ │   Waiting for its parent                  [ Accept ✕ ] [ Reject ]    ││
│ │ Change issue  #47 Decline message             Waiting for you        ││
│ │   Status  Todo → In progress                                         ││
│ │   Labels  + payments  − legacy                                       ││
│ │   Close   not planned                     [ Accept ] [ Reject ]      ││
│ └──────────────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the card is as wide as the comment. A row has a 1px `--border-1` border and `--r-sm` radius. Added text uses `--diff-add-bg` and removed text uses `--diff-del-bg`. A failed row uses `--danger` for its error text and a `Failed` word. All colours, radii, and motion come from the design-system tokens in `src/styles/colors_and_type.css`.

## UI contract boundary
- **Owned by the UI**: the card and its rows, the before and after rendering, the dependency display, the busy state, the keyboard order, the announcements, and the toasts.
- **Delegated to backend (abstract)**:
  - `"list map plan proposals (discussion id)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"accept map plan row (discussion id, proposal id, row key)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"reject map plan row (discussion id, proposal id, row key)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"map plan proposals changed"` — an event of `../core/MPR-map-plan-proposals.md`.

## Non-functional requirements
- A card of 20 rows renders without a visible delay.
- The card draws no raw HTML from a title or a body.
