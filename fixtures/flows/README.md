# Flow fixtures

Flow documents, schema version 2, that exist to be read by the TypeScript
implementation of the Flow schema (`src/state/flowDocument/`) against the
schema of `specifications/core/FGV-flow-graph-validation.md` (`FGV-FR-01`).

Every file here was written by `serializeFlowDocument`, so each is a body the
editor would actually produce. `src/state/flowFixtures.test.ts` sweeps this
directory: each file parses, and re-serializing what it parsed reproduces the
file byte for byte.

`Delivery.flow` uses every field of the schema: the six item kinds, both edge
types, Flow references, a draft link, footprints, a due date, a local-only
item, and issue and milestone sync links.
