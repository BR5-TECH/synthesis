/**
 * The libraries of a CALM read: the schema validator with the bundled CALM 1.3
 * schema (CLM-FR-KDTA, CLM-FR-BWGJ) and the layout engine (CLM-FR-GDML).
 *
 * This module is loaded with a dynamic import on the first read, so a file
 * with no CALM does not load these libraries.
 */
import Ajv2020, { type ErrorObject, type ValidateFunction } from "ajv/dist/2020";

import calmSchema from "@finos/calm-schema/schema/calm.json";
import calmTimelineSchema from "@finos/calm-schema/schema/calm-timeline.json";
import controlRequirementSchema from "@finos/calm-schema/schema/control-requirement.json";
import controlSchema from "@finos/calm-schema/schema/control.json";
import coreSchema from "@finos/calm-schema/schema/core.json";
import decoratorsSchema from "@finos/calm-schema/schema/decorators.json";
import evidenceSchema from "@finos/calm-schema/schema/evidence.json";
import flowSchema from "@finos/calm-schema/schema/flow.json";
import interfaceSchema from "@finos/calm-schema/schema/interface.json";
import timelineSchema from "@finos/calm-schema/schema/timeline.json";
import unitsSchema from "@finos/calm-schema/schema/units.json";

import type { ElkEngine } from "./layout";

/** CLM-FR-BWGJ: the schema that every document is validated against. */
export const CALM_SCHEMA_ID = "https://calm.finos.org/release/1.3/meta/calm.json";

/** CLM-FR-KDTA: the meta schema files of `@finos/calm-schema` 1.3.0. */
const SCHEMAS: object[] = [
  calmSchema,
  calmTimelineSchema,
  controlRequirementSchema,
  controlSchema,
  coreSchema,
  decoratorsSchema,
  evidenceSchema,
  flowSchema,
  interfaceSchema,
  timelineSchema,
  unitsSchema,
];

/** CLM-FR-OIGM: the most problems that a schema error lists. */
export const SHOWN_PROBLEMS = 5;

let validator: ValidateFunction | null = null;

/** The compiled validator of CALM 1.3, made once. */
function calmValidator(): ValidateFunction {
  if (validator) return validator;
  // The CALM schema uses keywords of its own vocabulary, which strict mode
  // refuses. The schema files are not changed (CLM-FR-KDTA).
  const ajv = new Ajv2020({ strict: false, allErrors: true });
  ajv.addFormat("date", /^\d{4}-\d{2}-\d{2}$/);
  for (const schema of SCHEMAS) ajv.addSchema(schema);
  const found = ajv.getSchema(CALM_SCHEMA_ID);
  if (!found) throw new Error(`the bundled schema ${CALM_SCHEMA_ID} is not known`);
  validator = found;
  return validator;
}

/** The combining keywords, whose own message adds nothing to their parts. */
const COMBINERS = new Set(["oneOf", "anyOf", "allOf", "if", "not"]);

/** CLM-FR-OIGM: the lines of a schema error, at most five and a count. */
export function problemLines(errors: readonly ErrorObject[]): string[] {
  const specific = errors.filter((error) => !COMBINERS.has(error.keyword));
  const chosen = specific.length > 0 ? specific : errors;
  const lines: string[] = [];
  const seen = new Set<string>();
  for (const error of chosen) {
    const line = `${error.instancePath || "/"}: ${error.message ?? error.keyword}`;
    if (seen.has(line)) continue;
    seen.add(line);
    lines.push(line);
  }
  if (lines.length <= SHOWN_PROBLEMS) return lines;
  const more = lines.length - SHOWN_PROBLEMS;
  return [
    ...lines.slice(0, SHOWN_PROBLEMS),
    `… and ${more} more ${more === 1 ? "problem" : "problems"}.`,
  ];
}

/** CLM-FR-BWGJ: null when the document matches CALM 1.3, else the problem lines. */
export function validateCalm(document: unknown): string[] | null {
  const validate = calmValidator();
  if (validate(document)) return null;
  return problemLines(validate.errors ?? []);
}

let engine: Promise<ElkEngine> | null = null;

/**
 * CLM-FR-GFZK: a layout engine in a web worker whose failure ends every
 * layout that waits for it. The worker of `elkjs` reports no error of its own,
 * so a worker that cannot start would leave each layout waiting for ever. After
 * a failure the engine is made again on the next read.
 */
export function workerEngine(
  make: (factory: (url?: string) => Worker) => ElkEngine,
  spawn: (url?: string) => Worker,
): ElkEngine {
  const waiting = new Set<(error: Error) => void>();
  const elk = make((url) => {
    const worker = spawn(url);
    worker.addEventListener("error", (event) => {
      event.preventDefault?.();
      engine = null;
      const failure = new Error("the layout worker failed");
      for (const fail of waiting) fail(failure);
      waiting.clear();
    });
    return worker;
  });
  return {
    layout: (graph) =>
      new Promise((resolve, reject) => {
        waiting.add(reject);
        elk.layout(graph).then(resolve, reject).finally(() => waiting.delete(reject));
      }),
  };
}

/**
 * The layout engine. In the application it runs in a web worker, so a large
 * document does not stop typing. Where no worker exists (the test runner), it
 * runs in this thread.
 */
export function layoutEngine(): Promise<ElkEngine> {
  if (engine === null) {
    engine = (async (): Promise<ElkEngine> => {
      if (typeof Worker === "undefined") {
        const { default: ELK } = await import("elkjs/lib/elk.bundled.js");
        return new ELK() as unknown as ElkEngine;
      }
      const [{ default: ELK }, { default: workerUrl }] = await Promise.all([
        import("elkjs/lib/elk-api"),
        import("elkjs/lib/elk-worker.min.js?url"),
      ]);
      return workerEngine(
        (factory) => new ELK({ workerUrl, workerFactory: factory }) as unknown as ElkEngine,
        (url) => new Worker(url ?? workerUrl),
      );
    })();
    engine.catch(() => {
      engine = null;
    });
  }
  return engine;
}
