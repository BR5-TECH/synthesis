# Run progress

**Spec code:** `RPV`

## Intent
The visualization a surface puts in front of an author watching long work that moves through named steps, so that a glance answers three questions — how far it has got, whether it is moving or waiting on something, and whether it has been round the same steps again. It exists because work of that shape is watched rather than waited on: an author who left the panel and came back needs the step, the condition, and the loop legible without reading a state name they would have to learn, and a bar that animates while nothing is happening tells them the opposite of the truth. It knows nothing about any particular work: the ordered steps, their labels, which one is current, what condition it is in, and where the work has looped are all passed to it by the surface that holds it, so the same visualization renders a graduation's four stages and any other host's stages on identical terms. Out of scope: deciding a stage or a condition, which is the host's from what its backend persisted; fetching, invoking, or subscribing to anything, which it does none of; and the detail behind a stage — an escalation's question, a revision's explanation, a publication's conflict, the log of what a stage did — which the surfaces holding it render around it or open from it, and which it only makes room for or reports an activation of.

## User stories
- As an author, I want to see which step a long run is on without reading a state name I would have to learn.
- As an author returning to a panel, I want to tell at once whether the run is working or waiting on me, because those two need different things from me.
- As an author whose run has gone back a step, I want to see that it looped and on which iteration, rather than watching a bar restart with no account of why.
- As an author using a screen reader or a keyboard alone, I want the same three answers the sighted pointer user gets.

## Wireframes

Four stages, the third of them working:

```
 ●━━━━━━━━━━●━━━━━━━━━━◐──────────○
 Queued     Authoring  Validation  Acceptance
 Done       Done       Working     —          iteration 3

 ━━━ = the track behind the current stage    ─── = the track ahead of it
```

The same configuration waiting on the author, and one that has looped:

```
 ●━━━━━━━━━━●━━━━━━━━━━●━━━━━━━━━━◑
 Queued     Authoring  Validation  Acceptance
 Done       Done       Done        Waiting for your review   iteration 3
```
```
 ●━━━━━━━━━━◐──────────○──────────○
 Queued     Authoring  Validation  Acceptance
 Done       Working    —           —          iteration 4
 ↩ Validation → Authoring on iteration 3
 ┌───────────────────────────────────────────────────────────────┐
 │  (detail the host places here)                                │
 └───────────────────────────────────────────────────────────────┘
```

A run that stopped without finishing:

```
 ●━━━━━━━━━━●━━━━━━━━━━◼──────────○
 Queued     Authoring  Validation  Acceptance
 Done       Done       Stopped     —
 Discarded by you on iteration 2
```

- Layout notes: the visualization is **one continuous track** running the whole width its host gives it, with the stage marks sitting on that track at even intervals in the configured order, each mark centred over the width its stage occupies and that stage's label and condition set beneath it on the same centre line. The row therefore begins and ends the same distance inside its two edges rather than stopping short of the trailing edge by the width of a label. The track is unbroken from the first mark to the last: no gap of the host's own surface shows between a mark and the track on either side of it, and the marks read as points on one line rather than as a row of separate dots. The track carries the completion of RPV-FR-05 along its own length — the length behind the current mark drawn in the complete treatment and the length ahead of it in the not-started one — so how far the work has got is legible from the track before a single label is read. The current mark is the one drawn most strongly, because which stage the work stands at is the first thing the row exists to say. The current stage's condition word is the one sentence the row is built around and is never the part that is dropped. The iteration rides at the trailing end of the condition line. A loop indication is its own line beneath the row, one per backward edge, naming the two stages and the iteration it happened on; a host that renders the same moves in its detail slot suppresses them, and the row carries none. The outcome statement takes that same position for work that has ended. The detail slot is the last thing in the component, beneath every line the component itself draws, so whatever the host puts there sits with the loop it explains rather than beside it. Nothing here scrolls horizontally at any width: at a width too narrow to hold every label, the current stage keeps its label and its condition whole and the others reduce to their marks. The component reserves the loop line's and the outcome line's height only when it has one to render.

## UI contract boundary
- **Owned by the UI**: the whole of it. The ordered rendering of the stages and their connectors; the derivation of complete, current, and not-started from the current stage's position in the configured order; the rendering of each of the six conditions and which of them may animate; the words each condition reads as and the accessible semantics beside them; the iteration label; the detection of a backward edge in the transition history and the loop indication it renders; the extent of progress and its bound at the end of the configured order; the outcome statement; the button semantics of an activatable stage, the reporting of its activation, and the announcing of why an inactivatable one is disabled; the detail slot and the fact that nothing in it is interpreted; the arrangement at every supported width; and the keyboard and assistive-technology exposure of all of it.
- **Delegated to backend (abstract)**: nothing. This component invokes no operation, subscribes to no event, and reads no store. Its whole input is what the host surface passes it on the render it is showing, and a host that passes nothing renders nothing.

## Functional requirements
1. **RPV-FR-01** This is a component rather than a surface. It is opened by nothing, occupies no zone of the main window (per `OVW-overview.md` OVW-FR-03), appears in no surface inventory (per `OVW-overview.md` OVW-FR-06), and renders only where a host surface places it.
2. **RPV-FR-02** Its whole configuration is an **ordered list of two or more stages**, each carrying a stable id, a short label, and the sentence assistive technology reads where the label alone is not one. The order of the list is the order rendered, and it is the only order any rule here is taken on.
3. **RPV-FR-03** It holds no knowledge of what a stage means. No stage id, label, or count is written into it, no behaviour is conditional on a particular id, and no configuration is more native to it than another: a host passing three stages, four, or seven renders on identical terms, with identical completion, condition, loop, and progress rules.
4. **RPV-FR-04** Beside the configuration it takes a **current stage**, naming one of the configured ids, and a **condition**, which is exactly one of six: `active`, `waiting`, `paused`, `blocked`, `stopped`, `complete`. Exactly one stage is current at a time. A current stage naming an id the configuration does not hold renders every stage as not started rather than guessing which was meant.
5. **RPV-FR-05** Completion is **positional**. Every stage before the current one in the configured order renders complete, the current one renders in its condition, and every stage after it renders not started. This holds under every condition, so a `stopped` current stage completes nothing after itself and work that ended without succeeding never reads as work that finished.
6. **RPV-FR-06** Every stage renders complete in exactly one state: the condition `complete` on the **last** configured stage. `complete` on any earlier stage completes that stage and the ones before it and leaves the rest not started, so there is no arrangement in which the work reads as wholly done before its last stage says so.
7. **RPV-FR-07** **Only `active` may animate.** Under `waiting`, `paused`, `blocked`, `stopped`, and `complete` nothing in the component moves, and none of the five renders, announces, or implies work in progress — a run resting on a decision the author has not taken is a run doing nothing, and a bar still travelling over it is the component saying otherwise.
8. **RPV-FR-08** Every condition is carried **in words on the stage it applies to and in accessible semantics**, never by a colour, a mark, or a motion alone. The host supplies the sentence a condition reads as — what is being waited for, what stopped it, what blocks it — and where it supplies none the condition's own name is read in its place, so a condition is never rendered without a word.
9. **RPV-FR-09** It takes an **iteration label** and renders it on the condition line. Where the host passes none, the line renders without one rather than with a zero.
10. **RPV-FR-10** It takes a **transition history**: an ordered list of entries each naming the stage moved from, the stage moved to, and the iteration the move was made on. An entry whose destination stands **earlier in the configured order** than its origin is a backward edge, and the component renders one loop indication per backward edge naming both stages and that iteration. A history holding no backward edge renders no loop indication and reserves no room for one. A host that renders the same moves itself, in the detail region of RPV-FR-13, says so and the component renders no indication and announces none, so the moves are read once where the host put them rather than twice on one surface.
11. **RPV-FR-11** **Progress is the current stage's position in the configured order and nothing else.** It is not accumulated across iterations, not derived from the length of the transition history, and not advanced by elapsed time. A stage reached for the second time renders at that stage's position exactly as it did the first time, so no history renders an extent past the end of the configured list and a repeated stage never reads as progress beyond the whole.
12. **RPV-FR-12** It takes an optional **outcome statement** — one line the host supplies for work that has ended — and renders it beneath the stages under the conditions `stopped` and `complete`. Under the other four it renders none, an outcome being a thing that has happened rather than a thing expected.
13. **RPV-FR-13** It renders whatever the host places in its **detail slot**, beneath everything the component itself draws, and interprets none of it: it parses nothing there, styles nothing there beyond the room it makes for it, and a detail the host omits leaves no gap. This is what lets the account of a loop sit with the loop it explains while the component stays ignorant of what any of it means.
14. **RPV-FR-14** The whole component is operable and readable from the keyboard alone. Nothing it renders is reachable by pointer only, nothing it renders depends on hover to be read, and where the component itself holds no control — which is every configuration in which no stage is activatable (RPV-FR-JSQW) — it takes no focus of its own rather than adding an empty stop to the host's tab order.
15. **RPV-FR-15** Assistive technology reads it as **one region** that names, in order: the configured stages, which of them are complete, which is current, that stage's condition together with the sentence the host supplied for it, the iteration, every loop indication, and the outcome statement where there is one. A change of current stage or of condition on a component already on screen is announced.
16. **RPV-FR-16** It renders at the smallest width its host offers, and it never scrolls horizontally. Where every label does not fit, the **current** stage keeps its label and its condition whole and the others reduce to their marks; it never reduces to a bar carrying no word at all, because the two things it exists to say are which stage and what condition.
17. **RPV-FR-17** It introduces no colour, icon, typography, or interaction pattern of its own. Its text is the **UI** typographic role (per `OVW-overview.md` OVW-FR-13), its status treatments are the host surface's own, and it therefore reads as part of whichever surface holds it, in the light and the dark theme alike (per `OVW-overview.md` OVW-FR-10).
18. **RPV-FR-18** It keeps nothing. It invokes no operation, subscribes to no event, reads no store, starts no timer other than the `active` animation, and retains no copy of what it was passed between renders, so what it shows is what its host passed for the render it is showing and never what it was passed before.
19. **RPV-FR-19** The stages are drawn as marks on **one continuous track** rather than as separate marks with space between them. The track runs unbroken from the first stage's mark to the last, every mark sits on it, no part of the host's own surface shows through between a mark and the track meeting it, and the marks are spaced evenly across the width the host offers so no stage reads as nearer to its neighbour than to another. Each mark sits at the **centre** of the width its stage occupies, with that stage's label and its condition set beneath it on the same centre line, so the track begins and ends the same distance inside the row's two edges rather than stopping short of the trailing edge by the width of a label. The track is the row's connective tissue: a run whose stages float apart says nothing about the order it moves in, which is the first of the three things this component exists to answer.
20. **RPV-FR-20** The track **carries completion along its length**. The stretch from the first mark to the current one is drawn in the same treatment a complete stage is drawn in, the stretch from the current mark to the last in the same treatment a not-started stage is drawn in, and the two meet at the current mark. Progress therefore reads from the track alone, and it is the positional completion of RPV-FR-05 and nothing else: it is not accumulated across iterations, not derived from the transition history, and not advanced by elapsed time (RPV-FR-11). Under `stopped` the stretch ahead of the current mark stays in the not-started treatment, so work that ended without finishing never reads as a full track.
21. **RPV-FR-21** Neither the track nor its completion introduces a colour, a mark, or a motion of its own: both take the host's own treatments on the terms of RPV-FR-17, both keep still under every condition but `active` (RPV-FR-07), and neither carries any meaning that is not also carried in words on the stage it belongs to (RPV-FR-08). At the smallest width the host offers, the track stays continuous and evenly spaced while the labels reduce as RPV-FR-16 already requires, so what narrows is what is written beneath the row rather than the row itself.

22. **RPV-FR-JSQW** A host may make a stage **activatable**. Beside each stage's id, label, and sentence, the configuration may carry whether that stage may be activated and, where it may not, the **sentence saying why**. A configuration in which no stage is activatable renders exactly as it does today and adds no control and no focus stop (RPV-FR-14).
23. **RPV-FR-LQUP** An activatable stage's entry — its mark, its label, and its condition together — is rendered with **keyboard-operable button semantics**: it is reachable in the host's tab order, it is activated by the keyboard as well as by the pointer, and it carries an accessible name naming its stage.
24. **RPV-FR-MAIP** A stage that is **not** activatable is rendered as a **disabled** entry rather than as no entry: it keeps its mark, its label, and its condition, and it **announces the sentence the host supplied for why it is disabled**. Where the host supplied none, it is rendered as plain text and is not announced as a control at all, so an entry is never a control with no account of why it does nothing.
25. **RPV-FR-NEVJ** Activating a stage **reports the activation to the host and does nothing else**. The component opens nothing, changes no current stage, changes no condition, and holds no selection of its own: what an activation means is the host's, exactly as the detail slot's content is (RPV-FR-13, RPV-FR-18).
26. **RPV-FR-RTQB** Which stages are activatable is **the host's data rather than this component's rule**. No stage id, position, or condition makes a stage activatable here, so the same configuration renders identically whichever host passed it (RPV-FR-03).
27. **RPV-FR-TZBL** Activation introduces **no colour, mark, or motion of its own**: an activatable entry, its focus ring, and its disabled treatment are the host's own, and what the entry is and whether it is disabled are carried in words and in accessible semantics rather than by a treatment alone (RPV-FR-17, RPV-FR-08).
28. **RPV-FR-WVPV** Activatable entries change nothing else about the row: completion is still positional (RPV-FR-05), only `active` still animates (RPV-FR-07), the track is still continuous and evenly spaced (RPV-FR-19), and at the smallest width the current stage still keeps its label and its condition whole while the others reduce to their marks (RPV-FR-16).

## Non-functional requirements
- Rendering costs what the configured stage count costs. A transition history of any length adds the loop indications it holds and nothing per entry beyond them, so a run that has looped twenty times is not a component that renders twenty times slower.
- The `active` animation respects the reduced-motion preference the operating system reports: where motion is reduced, the `active` condition is carried by its word and its semantics alone, which are what RPV-FR-08 already requires of every condition.
- Every part of it is legible in both themes at the smallest supported width, in the host's own tones. It brings no palette that would have to be re-checked when a host's changes.
- An activatable entry adds one focus stop per activatable stage and nothing per render, so a row of eight stages of which two may be opened costs two stops rather than eight.
- It is renderable in isolation from any configuration, with no application state, no project open, and no backend reachable, which is what makes a host's stage configuration testable as data rather than as behaviour.
