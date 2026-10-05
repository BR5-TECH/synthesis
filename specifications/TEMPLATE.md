# <Surface or feature title>

<!--
  This is the canonical template for specifications in the synthesis project.
  Every spec in every category below must follow it.

  Naming: `specifications/{ui,core,server,ai,tools,infra}/CODE-kebab-case-name.md`.
    - `ui/` specs cover the frontend surface (React, `src/**`).
    - `core/` specs cover the backend (Rust, `src-tauri/**`) and persistence/IPC contract.
    - `server/` specs cover the standalone server-side services (Rust, `server/**`)
      that run outside the desktop application. A `server/` spec owns its own crate,
      its HTTP surface, and its container image; it registers no Tauri command and is
      not reachable from `src/**` or `src-tauri/**`.
    - `ai/` specs cover the agent loops — how a conversation with a model is held:
      the prompt, the tools attached, the rounds taken, the calls repeated. A loop
      is reached from `core/`, which is where it is integrated into the application.
    - `tools/` specs cover the capabilities lent to a model inside a loop. Each is a
      `rig` portable tool; none registers a Tauri command or is reachable from `src/**`.
    - `infra/` specs cover build, CI, and test scaffolding rather than shipped behavior.
    - `CODE` is the spec's own 3-letter spec code (see below), in the same case
      as it appears on the `**Spec code:**` line, e.g. `SNV-shell-navigation.md`,
      `GSS-global-settings-storage.md`. The filename carries no ordering number;
      ordering and grouping live inside the specs, not in the filename.

  Spec code:
    - Every spec has a globally-unique 3-letter code, derived from the first
      letters of the feature name (e.g. Project picker -> PPK, Dashboard -> DSH).
      It is declared on a `**Spec code:** `XXX`` line immediately under the H1,
      and it is also the filename prefix (see Naming above).
    - When two specs would collide (e.g. a paired ui/ + core/ feature such as
      Git, Search, History), the UI surface keeps the natural code and the
      backend anchor takes a variant (GIT/GTC, SCH/SCC, HVW/HIS).
    - The code prefixes every requirement ID in the spec, making every ID
      globally unique on its own.

  IDs inside a spec (XXX = this spec's code):
    - A specification defines requirements and nothing else. Every new
      functional requirement is XXX-FR- plus four random capital letters, e.g.
      XXX-FR-QJZM. Draw the letters at random from A-Z, then search the spec
      and draw again if that ID is taken.
    - An ID says which requirement, never which position. It holds no number
      and no order, so adding or removing a requirement moves no other ID.
    - IDs already in the corpus are numeric (XXX-FR-01). They keep their
      numbers for good. Both forms are valid, one spec may hold a mix, and
      neither is ever converted to the other.
    - Never renumber, never re-letter, never reuse. A deleted requirement
      retires its ID and leaves a gap, which is expected.
    - The one exception is a SPLIT: when a spec grows too large to read and is
      divided into several specs, each requirement that moves takes the new
      spec's code and keeps its own suffix (GRD-FR-45 -> GQS-FR-45). The suffix
      is never redrawn, no requirement is retired, and every citation in the
      corpus and in the code is rewritten in the same change. A split is the
      only reason an ID is ever re-lettered; nothing else may.
    - There are no XXX-TS- identifiers. A test scenario is a test. The
      requirements ARE the acceptance criteria, and the test that verifies one
      names it — in the test title for TypeScript, in the comment above the
      test function for Rust. `tools/spec-check/spec_check.py --coverage`
      reports which requirements no test names.

  Cross-references between specs use backticked filenames followed by the
  target spec's prefixed ID, e.g. see `SNV-shell-navigation.md` SNV-FR-09.

  Sections marked OPTIONAL may be omitted if they would be empty.
  All other sections are required.

  Section order: Intent, then Functional requirements, then everything else —
  user stories, wireframes, the contract surface, and the non-functional
  requirements. Requirements come near the top because a reader who reaches a
  spec part-way through must reach what it requires before they reach how it
  is shaped; a spec that opens with several hundred lines of record shapes
  reads, to anything that samples it, as a spec holding no requirements at all.

  Write every spec as a description of the target state — what the surface or
  feature IS once built. Do not narrate history: no "previously X, now Y", no
  "changed from", no "we considered A but chose B", no changelog. A reader
  should not be able to tell from the text whether the spec was written from
  scratch or edited ten times. This holds for brownfield edits too: fold the
  change into the existing prose so the result reads as one clean target state.

  Write in ASD-STE100 Simplified Technical English throughout. One idea per
  sentence, the active voice, a short sentence in preference to a long one,
  and one term for one thing. A specification is read far more often than it
  is written, and most of its readers sample it rather than read it end to end.
-->

**Spec code:** `XXX`

## Intent
<!--
  2–4 sentences. What this surface or feature IS, who it's for, what problem
  it solves. Capture motivation, not implementation. A reader who has never
  seen the project should be able to tell from this paragraph alone whether
  this spec is relevant to a question they have.
-->

## Functional requirements
<!--
  Each new ID is XXX-FR- plus four random capital letters. The list number is
  markdown numbering alone and is not part of the ID.

  A requirement is ONE testable assertion, at most 60 words, in Simplified
  Technical English. It states what the feature does, not why. If a statement
  needs a subordinate clause to defend itself, it is a requirement plus a
  reason: move the reason out.

  The cap governs every requirement you write or rewrite. It does NOT license
  splitting an existing one to meet it: a split leaves every citation of that
  requirement pointing at less than it cited, and there are tens of thousands
  of citations. `spec_check.py --concision` reports the requirements the
  corpus still holds over the cap; they come down as their specs are edited.

  The reason is CUT, not relocated. A specification states what is true, and
  a paragraph defending each requirement is the single largest thing that made
  this corpus unreadable. Delete the justification.

  The exception is a reason that is LOAD-BEARING: one without which a reader
  would implement the requirement wrongly, or would read it as arbitrary and
  quietly "fix" it. That one goes on a `*Why:*` sub-bullet — ONE sentence, at
  most 40 words. Expect to keep one for roughly one requirement in five. If
  the reason only restates the requirement or says why it is nice, it goes.

  Reference other specs in backticks when behavior depends on them, e.g.
  "(per `SNV-shell-navigation.md` SNV-FR-09)".

  `tools/spec-check/spec_check.py` enforces the caps.
-->
1. **XXX-FR-QJZM** <one testable assertion, at most 60 words>.
   - *Why:* <one sentence, at most 40 words. Omit unless the reason is non-obvious.>
2. **XXX-FR-BKPT** <statement>.

## User stories
<!-- OPTIONAL. Use when persona-driven motivation clarifies scope.
  - As a <role>, I want to <action> so that <benefit>.
-->

## Wireframes
<!-- OPTIONAL. ASCII layouts or links to images. Include "layout notes"
  bullets after the wireframe(s) describing constraints the renderer must
  satisfy (column ratios, flexible regions, where errors attach, etc.).
  Only applies to UI specs.
-->

## UI contract boundary
<!--
  REQUIRED for UI specs. For core, server, and every other non-UI spec, replace
  with "## Contract surface" describing what that module or service exposes —
  its operations and payload shapes, or, for a server spec, its crate, its
  configuration, its HTTP surface, and its image.

  For UI specs:
    - **Owned by the UI**: what the frontend renders, decides, and validates
      client-side. Layout, input shape, button enablement, etc.
    - **Delegated to backend (abstract)**: named operations the UI invokes.
      These names are the contract surface and MUST match operation names
      defined in the corresponding `core/` spec byte-for-byte. If no `core/`
      spec exists yet, mark the operation inline as a stub, e.g.
      `open_project_at_path (stub — no core spec yet)`.
-->

## Non-functional requirements
<!--
  Bulleted, free-form. Performance, persistence assumptions, network
  posture, accessibility floors, platform constraints.

  A statement here is one no test names. Anything testable and tested belongs
  in Functional requirements, where a test can cite it.
-->
- <statement>.
