# Flow fixtures

Flow documents that exist to be read by both implementations of the Flow schema
— `src/state/flowDocument.ts` on the TypeScript side and
`src-tauri/src/flow_validation.rs` on the Rust side (`FGV-FR-01`).

Every file here was written by `serializeFlowDocument`, so each is a body the
editor would actually produce. Two tests sweep this directory and
`resources/flows/` together, from opposite sides of the contract:

- `src/state/flowFixtures.test.ts` — each file parses, and re-serializing what it
  parsed reproduces the file byte for byte.
- `src-tauri/src/flow_validation.rs::every_committed_flow_validates` — each file
  validates with no violations.

A shape one side gains and the other does not then fails a test rather than
waiting to be found by an author whose Flow will not save. `resources/flows/`
holds this project's own workflows and covers what they happen to use; this
directory covers the rest — today, the whole of the loop half of the schema,
which no real workflow here uses yet.
