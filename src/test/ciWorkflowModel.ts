/**
 * The CI workflows, parsed once and described as data.
 *
 * `./ci-workflow.test.ts` asserts against this model. The reading and the
 * asserting are separated because the model is the larger half and neither
 * half is easier to follow beside the other.
 */
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { posix, resolve } from "node:path";
import { parse } from "yaml";

/**
 * Conformance tests for the CI pipeline (`specifications/infra/CIP-ci-pipeline.md`).
 *
 * The pipeline's requirements are structural properties of a single YAML file,
 * and their failure modes are silent: a `paths` filter or a missing `always()`
 * does not break anything visibly — it leaves pull requests pending forever,
 * and a stray `continue-on-error` turns a red lane green. These tests turn that
 * class of drift into a red build.
 */

// Vitest runs with the project root as cwd; import.meta.url is not a file:
// URL under the jsdom environment.
export const ROOT = process.cwd();
export const WORKFLOW_DIR = resolve(ROOT, ".github/workflows");
export const WORKFLOW_PATH = resolve(WORKFLOW_DIR, "ci.yml");

export const source = readFileSync(WORKFLOW_PATH, "utf8");
export const workflow = parse(source);

/**
 * YAML 1.1 folds a bare `on` key to boolean true; 1.2 keeps it a string.
 *
 * The parsed document is untyped, as the rest of this file treats it: these
 * tests assert its shape rather than assuming one.
 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export const triggersOf = (doc: any): any => doc.on ?? doc[true as unknown as string];
export const triggers = triggersOf(workflow);

/**
 * Every workflow the repository holds, parsed. CIP-FR-01 is a property of the
 * whole directory rather than of one file: the verification workflow is the
 * only one that may report a pull-request check, and the publishing workflow
 * (CIP-FR-24) sits beside it reporting none.
 */
export const WORKFLOW_FILES: string[] = readdirSync(WORKFLOW_DIR).sort();
export const workflowsByFile = new Map(
  WORKFLOW_FILES.map(
    (file) =>
      [file, parse(readFileSync(resolve(WORKFLOW_DIR, file), "utf8"))] as const,
  ),
);

/** The publishing workflow (CIP-FR-24). */
export const PUBLISH_FILE = "server-image.yml";
export const publishSource = readFileSync(resolve(WORKFLOW_DIR, PUBLISH_FILE), "utf8");
export const publishWorkflow = parse(publishSource);

/**
 * The system packages the backend lane installs for Tauri. A lane that started
 * installing them would mean its crate had grown a dependency it is specified
 * not to have, and the apt step would hide that rather than surface it.
 */
export const TAURI_PACKAGES = [
  "libwebkit2gtk-4.1-dev",
  "libayatana-appindicator3-dev",
  "librsvg2-dev",
  "libxdo-dev",
];

export const GATE = "gate";
/**
 * Derived, never hardcoded: CIP-FR-16 promises that adding a lane is two edits
 * to ci.yml and nothing else. A literal lane list here would silently make it
 * three and invert the requirement this file claims to protect.
 */
export const LANES: string[] = Object.keys(workflow.jobs).filter((id) => id !== GATE);
export const ALL_JOBS: string[] = Object.keys(workflow.jobs);

export type Step = {
  uses?: string;
  run?: string;
  if?: string;
  with?: Record<string, unknown>;
  env?: Record<string, unknown>;
  "working-directory"?: string;
  "continue-on-error"?: unknown;
};

export const stepsOf = (jobId: string): Step[] => workflow.jobs[jobId].steps ?? [];

/** Flattens a job's steps into the shell text they run. */
export const runScripts = (jobId: string): string =>
  stepsOf(jobId)
    .map((step) => step.run ?? "")
    .join("\n");

/**
 * Folds the spellings of one directory together, so rewriting `src-tauri` as
 * `./src-tauri` — a plausible tidy-up, semantically identical to Actions —
 * cannot flip a containment check and fail the suite.
 */
export const normaliseDir = (dir: string): string =>
  posix.normalize(dir).replace(/^\.\//, "").replace(/\/+$/, "") || ".";

/** The directory a job's `run:` steps execute in, defaulting to the root. */
export const workingDirOf = (jobId: string): string =>
  normaliseDir(workflow.jobs[jobId].defaults?.run?.["working-directory"] ?? ".");

/** The directory one step runs in: its own override, else the job's default. */
export const stepDirOf = (jobId: string, step: Step): string =>
  normaliseDir(step["working-directory"] ?? workingDirOf(jobId));

/** Resolves a path a step names against the directory that step runs in. */
export const resolveFrom = (dir: string, path: string): string =>
  normaliseDir(dir === "." ? path : `${dir}/${path}`);

/** True when `child` is `parent` or sits beneath it. */
export const isInside = (child: string, parent: string): boolean =>
  parent === "." || child === parent || child.startsWith(`${parent}/`);

/** The `uses:` action refs a job pulls in, in order. */
export const actionRefs = (jobId: string): string[] =>
  stepsOf(jobId)
    .map((step) => step.uses)
    .filter((ref): ref is string => typeof ref === "string");

/** The gate's guard — the step that actually fails the job. */
export const gateGuard = (): Step | undefined =>
  stepsOf(GATE).find((step) => step.run?.includes("exit 1"));

/** The repository's single Rust pin (CIP-FR-10). */
export const PIN = "src-tauri/rust-toolchain.toml";
export const PIN_FILE = posix.basename(PIN);
export const PIN_FILE_PATTERN = PIN_FILE.replace(/\./g, String.raw`\.`);

/**
 * The command each statement in a script invokes. Statements, not raw text:
 * matching a bare word anywhere in a `run:` block would classify a lane by its
 * prose, so `echo "no cargo needed here"` would reclassify the `ui` lane as a
 * Rust one and fail half this file on a comment edit.
 */
export const commandsOf = (run: string): string[] =>
  run
    .split(/[\n;&|]+/)
    .map((statement) => statement.trim())
    .filter((statement) => statement && !statement.startsWith("#"))
    // Drop `sudo` and any leading VAR=value assignments to reach the command.
    .map((statement) =>
      statement
        .split(/\s+/)
        .find((word) => word !== "sudo" && !/^\w+=/.test(word)),
    )
    .filter((command): command is string => Boolean(command));

/**
 * Commands whose behaviour the pinned toolchain decides. `cargo-nextest` and
 * friends are included deliberately: a lane running only those is still a Rust
 * lane, and a predicate that missed it would grant it no pin coverage at all.
 */
export const TOOLCHAIN_COMMAND = /^(cargo|cargo-[\w-]+|rustc|rustup|rustfmt|clippy-driver)$/;

/** Actions that resolve, install, or probe a toolchain. */
export const TOOLCHAIN_ACTION = /rust|cargo|toolchain/i;

export const invokesToolchain = (step: Step): boolean =>
  commandsOf(step.run ?? "").some((command) => TOOLCHAIN_COMMAND.test(command));

/**
 * The Rust lanes, discovered from what they run rather than listed, so a lane
 * added later inherits the toolchain and caching guarantees below with no edit
 * here (CIP-FR-16).
 */
export const RUST_LANES: string[] = LANES.filter((lane) =>
  stepsOf(lane).some(invokesToolchain),
);

/**
 * Index of the first step whose behaviour the pin decides. An index, not an
 * offset into the concatenated shell text: `uses:` steps run no shell, so an
 * offset-based order is blind to a cache action — which probes `rustc -vV` —
 * being moved ahead of the step that puts the pin in place.
 */
export const firstToolchainStep = (jobId: string): number => {
  const at = stepsOf(jobId).findIndex(
    (step) => invokesToolchain(step) || TOOLCHAIN_ACTION.test(step.uses ?? ""),
  );
  return at < 0 ? Number.POSITIVE_INFINITY : at;
};

/** The rust-cache step a lane pulls in, if any. */
export const cacheStepOf = (jobId: string): Step | undefined =>
  stepsOf(jobId).find((step) => step.uses?.startsWith("Swatinem/rust-cache"));


/**
 * Where a lane copies the pin to, as a repository-relative path, or undefined
 * if it never copies it. Matched as a copy *command* rather than as a mention
 * of the path, so a deleted step that left its explanatory comment behind — the
 * likeliest regression of all — does not read as a copy that still happens.
 */
export type PinCopy = { step: Step; index: number; destination: string };
export const pinCopyOf = (jobId: string): PinCopy | undefined => {
  // The commands that take `<source> <destination>` in that order. A copy
  // written some other way — a composite action, a redirect — reads as no copy
  // and fails loudly here rather than passing silently, which is the safer
  // direction for a guard whose whole purpose is catching a missing copy.
  const COPY = String.raw`(?:cp|install|rsync)`;
  for (const [index, step] of stepsOf(jobId).entries()) {
    const copy = new RegExp(
      String.raw`(?:^|[;&|]|\n)\s*${COPY}\s+(?:-\S+\s+)*(\S*${PIN_FILE_PATTERN})\s+(\S+)`,
    ).exec(step.run ?? "");
    if (!copy) continue;

    const dir = stepDirOf(jobId, step);
    if (resolveFrom(dir, copy[1]) !== PIN) continue;

    // A destination naming a directory means the file keeps its name.
    const named = copy[2].endsWith("/") ? `${copy[2]}${PIN_FILE}` : copy[2];
    return { step, index, destination: resolveFrom(dir, named) };
  }
  return undefined;
};

/** True for a path that lies inside the repository, so git can be asked about it. */
export const isRepoRelative = (path: string): boolean =>
  !path.startsWith("/") && !path.startsWith("../");

/**
 * Repository-relative paths git tracks, for a pathspec, or undefined when this
 * tree has no git to ask — a source tarball, or a build context copied without
 * `.git`. Undefined rather than a throw: these assertions are worth skipping
 * there, not worth erroring the whole file out at collection time.
 */
export const tracked = (pathspec: string): string[] | undefined => {
  if (!isRepoRelative(pathspec)) return undefined;
  try {
    return execFileSync("git", ["ls-files", "-z", "--", pathspec], {
      cwd: ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .split("\0")
      .filter(Boolean);
  } catch {
    return undefined;
  }
};

/**
 * Crate directories, found by a bounded filesystem walk rather than by asking
 * git, so discovery works in a tree with no `.git` and never descends into a
 * `target/` or `node_modules/`.
 */
export const SKIP_DIRS = new Set(["node_modules", "target", "dist", "gen"]);
export const crateDirs = (dir: string, depth: number): string[] => {
  if (depth < 0) return [];
  const entries = readdirSync(resolve(ROOT, dir), { withFileTypes: true });
  const here =
    dir !== "." && entries.some((e) => e.isFile() && e.name === "Cargo.toml")
      ? [dir]
      : [];
  return [
    ...here,
    ...entries
      .filter(
        (e) => e.isDirectory() && !SKIP_DIRS.has(e.name) && !e.name.startsWith("."),
      )
      .flatMap((e) => crateDirs(dir === "." ? e.name : `${dir}/${e.name}`, depth - 1)),
  ];
};

/** Crates sit at most a couple of directories down; `tools/<crate>` is the deepest. */
export const CRATE_DIRS: string[] = crateDirs(".", 3);
