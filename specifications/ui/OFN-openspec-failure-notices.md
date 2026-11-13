# OpenSpec failure notices

**Spec code:** `OFN`

## Intent
A **failure notice** is the one way the frontend states an OpenSpec failure. Each failure class of `../core/OFP-openspec-failure-policy.md` has one behaviour, so each class has one set of actions here, and each failure code has one sentence, wherever the author meets it: in the change workspace, in a start dialog, in a run region, on a stream row, or on the Spec Map. Beside our sentence, the author can open the CLI's own messages, which say exactly what OpenSpec found. Out of scope: the classification itself and the content of the diagnostics, which are `../core/OFP-openspec-failure-policy.md`'s; the frame of each surface around the notice, which is that surface's spec; the OpenSpec section of Project settings, which is `SET-project-settings.md`'s; and the refusal texts that agent tools give to a model, which the backend fixes.

## Functional requirements
1. **OFN-FR-RNWG** The frontend renders every OpenSpec failure through one failure notice. The notice reads the classifying code of an `OpenSpecFailure`, its `cause` where present and its `code` otherwise (per `../core/OFP-openspec-failure-policy.md` OFP-FR-BYYT), and takes the class and the sentence of that code from the table of the contract boundary.
2. **OFN-FR-PVYE** The notice table names each code of the class table of `../core/OFP-openspec-failure-policy.md` with the same class. A code that the table does not name gets no notice, and its surface states it on its own terms.
3. **OFN-FR-EPIG** A notice of the class `environment` offers **Open OpenSpec settings**, which opens the OpenSpec section of Project settings (per `SET-project-settings.md` SET-FR-FZKL). A notice of `openspec_image_cli_unavailable` offers **Open Docker settings**, which opens the Docker section (SET-FR-21), in its place.
4. **OFN-FR-CBLG** A notice of the class `transient` offers **Retry**. Retry invokes `"get openspec status"` with `recheck` set, and then invokes the operation that failed again with the same arguments. While the two calls are in flight, Retry is disabled.
5. **OFN-FR-RLSO** A notice of the class `content` offers **Open change**, which opens the change workspace tab of the change in the location of the failure (per `OCW-openspec-change-workspace.md` OCW-FR-DMGL) and selects `threadId` where the failure carries one (OCW-FR-QSZP). A notice on the tab of that change offers no Open change.
6. **OFN-FR-RCWY** A notice whose failure carries `repairRunId` also offers **Open run**, which opens the Runs bottom panel on its graduation section and selects that run (per `GRU-graduation-runs.md` GRU-FR-PAHN). Its sentence adds that a rework run started to repair the change.
7. **OFN-FR-LRCZ** A notice whose failure carries `repairRefusal` adds a sentence that no repair run started, with the refusal in words: `proposal_pending` that the change holds a pending edit proposal, `openspec_run_active` that a run of the change is active, an image refusal that the agent image is not ready, and every other code by its code.
8. **OFN-FR-DPIJ** A notice on a surface that has its own retry of the failed operation offers no Retry: the Continue of a run region (per `GRU-graduation-runs.md` GRU-FR-LBPR) and the Retry of the Spec Map (per `SMP-specification-map.md` SMP-FR-KBXW).
9. **OFN-FR-IHSQ** Where the failure carries at least one diagnostic, the notice offers **Details**, a disclosure that is closed when the notice appears. Open, it lists each diagnostic in order on its own line, as plain text: the severity, the code, and the path, each where present, then the message, and the fix on the next line after `Fix:`.
10. **OFN-FR-UNEF** Details renders no Markdown, no HTML, and no link, and it scrolls on its own above a height of 12 lines. A diagnostic text that holds a control character renders that character as a space.
11. **OFN-FR-DZSQ** Details offers **Copy**, which writes every diagnostic of the notice to the clipboard as plain text, in the lines of OFN-FR-IHSQ (a fix on the line after its diagnostic), and then states for 2 seconds that the text is copied.
12. **OFN-FR-QZEC** The **short form** of a notice is its sentence alone, with no action and no Details. A surface that has no room for actions, such as a run strip or a stage sentence, uses the short form.
13. **OFN-FR-PFYW** A surface shows one notice for one failure, and no other sentence of its own that states the same failure. The frame sentence of the surface, such as that no run started or that the base branch did not move, is not such a sentence.
14. **OFN-FR-ONNF** A notice that appears is announced once to assistive technology. Details is a button that carries its open state, and every action is reachable with the keyboard alone.
15. **OFN-FR-LKAT** Each notice that appears emits through `../core/LGC-logging.md`'s frontend helpers under the `frontend` domain one `WARN` record with the surface, the code, the cause, and the class. No record holds a diagnostic, a `detail` other than a CLI state, or a change name.
16. **OFN-FR-HEVB** The notice is used by these surfaces: the reads and the starts of the change workspace (per `OCW-openspec-change-workspace.md` OCW-FR-RFGG, OCW-FR-PFUQ, OCW-FR-EKYF, OCW-FR-CTWP, OCW-FR-BTSI), the propose start (per `GSD-graduation-start-dialog.md` GSD-FR-RFIG), the run region (per `GRU-graduation-runs.md` GRU-FR-LBPR, GRU-FR-BPNJ, GRU-FR-PXCW), the stream row (per `WSS-work-stream-selector.md` WSS-FR-NTGC), and the Spec Map (per `SMP-specification-map.md` SMP-FR-KBXW, SMP-FR-KVKD).
17. **OFN-FR-KMPW** Details that open inside a scrolled container, such as the merge confirmation of the stream list, scroll into view in that container. Details that are already in view move nothing.

## Wireframes

```
┌ OpenSpec ───────────────────────────────────────────────────────────────────┐
│ The deltas of "add-logout" do not apply to the live specifications.          │
│ A rework run started to repair the change.                                   │
│ [Open change] [Open run]                                    [Details ▾]      │
│ ┌──────────────────────────────────────────────────────────────── [Copy] ┐  │
│ │ error archive_spec_update_failed: MODIFIED "Session Expiration" not     │  │
│ │ found in auth                                                           │  │
│ │ Fix: Use ADDED for a new requirement                                    │  │
│ └─────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
```

- Layout notes: the sentence wraps, and the actions stand on one line below it where they fit and wrap where they do not. Details opens below the actions at the full width of the notice. A long diagnostic wraps at a word and never one letter to a line.

## UI contract boundary
- **Owned by the UI**: the notice, its table of sentences and actions, the routes of its actions, the Retry sequence, the Details disclosure and its Copy, the short form, the announcement, and the log record (OFN-FR-RNWG through OFN-FR-HEVB).
- **Delegated to backend (abstract)**:
  - "get openspec status" (`get_openspec_status(recheck)`), owned by `../core/OSM-openspec-mode.md` (OSM-FR-YXUC). The operation that a Retry invokes again is delegated by the spec of the surface that shows the notice.

### Notice table
The sentence of a code. `<change>` is the `change` of the failure, and `<artifacts>` is its `detail`.

```text
code                             detail        class        sentence
openspec_mode_inactive                         environment  OpenSpec mode is off for this project.
cli_unavailable                  not_found     environment  The OpenSpec CLI is not installed on this machine.
cli_unavailable                  unsupported   environment  The OpenSpec CLI on this machine is too old. OpenSpec mode needs version 1.0 or later.
cli_unavailable                  failed        environment  The OpenSpec CLI on this machine does not start or does not report its version.
cli_output_unreadable                          environment  The OpenSpec CLI gave an answer that the application cannot read. Check the version of the OpenSpec CLI.
openspec_image_cli_unavailable   not_found     environment  The agent image of this project does not hold the OpenSpec CLI.
openspec_image_cli_unavailable   unsupported   environment  The OpenSpec CLI in the agent image of this project is too old. It must be version 1.0 or later.
openspec_image_cli_unavailable   failed        environment  The OpenSpec CLI in the agent image of this project does not start. The image needs Node 22.12.0 or later.
cli_timed_out                                  transient    The OpenSpec CLI did not finish in time, also after one more try.
cli_failed                                     transient    The OpenSpec CLI stopped before it answered, also after one more try.
cli_rejected                                   content      OpenSpec reported a problem in the OpenSpec files of this change.
change_not_ready                               content      The change misses these artifacts: <artifacts>. Complete them before Apply.
tasks_incomplete                               content      The change "<change>" has tasks that are not done.
archive_tasks_incomplete                       content      The change "<change>" has tasks that are not done.
archive_spec_update_failed                     content      The deltas of "<change>" do not apply to the live specifications.
archive_spec_validation_failed                 content      The live specifications are not valid after the deltas of "<change>".
archive_validation_failed                      content      The change "<change>" is not valid.
archive_target_exists                          content      An archive folder for "<change>" with the date of today exists already.
archive_error                                  content      OpenSpec could not archive "<change>".
any other archive code                         content      OpenSpec could not archive "<change>".
```

## Non-functional requirements
- A notice costs no backend call until the author acts on it. Retry costs one status call and one call of the failed operation.
- The notice table and the class table of `../core/OFP-openspec-failure-policy.md` are the same list of codes, so a code added on one side is added on the other in the same change.
