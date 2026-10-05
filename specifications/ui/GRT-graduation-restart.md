# Graduation restart

**Spec code:** `GRT`

## Intent
Starting a new run over the prompt a discarded run carried. A restart answers the prompt the discarded run captured, on a work stream the author chooses at the moment they take it, and leaves the discarded run exactly as it stands. It is offered from the discarded run's row and from its region, it confirms before it invokes anything, and both runs afterwards say what they are to each other. Out of scope: the rail the control stands in, which is `GRH-graduation-history.md`'s; what either run renders once selected, which is `GRU-graduation-runs.md`'s; and the run record a restart creates, which is `../core/GRD-graduation.md`'s.

## Functional requirements
1. **GRT-FR-CTNO** **Restart** is offered for a run in `discarded` and for no other state, and it stands in exactly two places: the trailing edge of the run region's stage row, where **Discard run** stands for a non-terminal run, and the history rail row's action cluster, where **Archive** stands.
2. **GRT-FR-AMHS** The two controls are **one act reachable from two places**. They carry the same accessible name, the same tooltip, the same disabled behaviour and the same inline-error behaviour. There is one confirmation: the rail's **Restart** selects the discarded run and opens that confirmation in the run region.
3. **GRT-FR-VWHM** Restart **confirms before invoking anything**. The confirmation names the run, states that a new run is created and that the discarded run stays discarded and unchanged, and, for a stream run, requires the author to **choose the work stream** the new run will run on.
4. **GRT-FR-IMRI** The stream choice offers every live stream of the project and defaults to the stream the discarded run ran on, where that stream still exists. A stream a run currently holds is offered, because the new run waits in that stream's queue rather than starting at once.
5. **GRT-FR-SQNB** The confirmation asks the **standing-work choice** and the **commit message** on the terms the start dialog asks them (per `GSD-graduation-start-dialog.md` GSD-FR-TBQX, GSD-FR-WQPD, GSD-FR-MZTB), and both travel with `restart_graduation_run`. It rests at the same positions for every restart, and it renders no path set. The commit message field starts empty: the default of the start dialog's commit message (per `GSD-graduation-start-dialog.md` GSD-FR-HVDN) belongs to new starts alone, and the restart reads no queue for it.
6. **GRT-FR-KSBC** Every typed refusal the operation returns is rendered inline against the discarded run (per `GRU-graduation-runs.md` GRU-FR-XQVG), states that no new run was created and that this run is unchanged, and leaves **Restart** offered. Each says what clears it, and this surface retries nothing by itself.
7. **GRT-FR-FCUF** Restart provenance is rendered on both runs from the records the section already holds. A run carrying `restarted_from_run_id` states which run it was restarted from and offers a route that selects it; a discarded run states each run restarted from it. A route changes the selection alone and starts nothing.
8. **GRT-FR-XHLN** An accepted restart selects the new run it created, also where the author selected another run while the restart ran. The rail reveals that run where a filter hides it (per `GRH-graduation-history.md` GRH-FR-XBWU), and keyboard focus goes to its first action-row control (per `GRU-graduation-runs.md` GRU-FR-PVXD). The discarded run stays unchanged and selectable.
9. **GRT-FR-NPDC** The confirmation of a discarded **direct** run names its pinned worktree and branch and states that the new run works there directly. It asks no stream and no standing-work choice, and it invokes `restart_graduation_run` with resting values for both.

## UI contract boundary

**Owned by the UI**: which runs offer the control, the confirmation and the stream choice within it, the default stream, every inline refusal, and the provenance wording on both runs.

**Delegated to backend (abstract)**:
- `restart_graduation_run`
- `list_work_streams`

## Non-functional requirements
- A restart costs one call and one re-list. The confirmation is composed from the run record the section already holds, so opening it reads nothing but the stream list.
- Apart from the selection of the new run and the filter change that revealing it needs, nothing about the act moves the author: a filter that admits the new run, the rail's scroll position and its width are exactly as they were.
