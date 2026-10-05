import { readFileSync } from "node:fs";
import { posix, resolve } from "node:path";
import { describe, expect, it } from "vitest";

import {
  ALL_JOBS,
  CRATE_DIRS,
  GATE,
  LANES,
  PIN,
  PIN_FILE,
  PIN_FILE_PATTERN,
  PUBLISH_FILE,
  PinCopy,
  ROOT,
  RUST_LANES,
  SKIP_DIRS,
  Step,
  TAURI_PACKAGES,
  TOOLCHAIN_ACTION,
  TOOLCHAIN_COMMAND,
  WORKFLOW_DIR,
  WORKFLOW_FILES,
  WORKFLOW_PATH,
  actionRefs,
  cacheStepOf,
  commandsOf,
  crateDirs,
  firstToolchainStep,
  gateGuard,
  invokesToolchain,
  isInside,
  isRepoRelative,
  normaliseDir,
  pinCopyOf,
  publishSource,
  publishWorkflow,
  resolveFrom,
  runScripts,
  source,
  stepDirOf,
  stepsOf,
  tracked,
  triggers,
  triggersOf,
  workflow,
  workflowsByFile,
  workingDirOf,
} from "./ciWorkflowModel";
import { TEST_AREAS } from "./testAreas";

/** The frontend test lane. Its areas are one lane (CIP-FR-VRPM). */
const UI_TESTS = "ui-tests";
/** The two frontend lanes, which provision and cache the same way. */
const UI_JOBS = ["ui", UI_TESTS];

describe("CIP-FR-01 workflow identity", () => {
  it("is named CI", () => {
    expect(workflow.name).toBe("CI");
  });

  /**
   * CIP-FR-01 makes this the only *pull-request verification* workflow, not the
   * only file in the directory: publication is a separate workflow that reports
   * no pull-request check (CIP-FR-24). Asserted over every file present, so a
   * workflow that started reporting a check on a pull request — a second check
   * name that branch protection knows nothing about — fails here.
   */
  it("is the only workflow that runs on a pull request", () => {
    const reportsAPullRequestCheck = WORKFLOW_FILES.filter((file) => {
      const on = triggersOf(workflowsByFile.get(file)) ?? {};
      return "pull_request" in on || "pull_request_target" in on;
    });
    expect(reportsAPullRequestCheck).toEqual(["ci.yml"]);
  });

  // Two workflows sharing a name would collide on check names.
  it("shares its name with no other workflow", () => {
    const names = WORKFLOW_FILES.map((file) => workflowsByFile.get(file).name);
    expect(new Set(names).size).toBe(names.length);
  });
});

describe("CIP-FR-02 triggers", () => {
  it("runs on pull requests with the default activity types", () => {
    expect(Object.keys(triggers)).toEqual(["pull_request"]);
    expect(triggers.pull_request.types).toEqual([
      "opened",
      "synchronize",
      "reopened",
    ]);
  });

  it("declares no push trigger", () => {
    expect(triggers.push).toBeUndefined();
  });

  // A path-filtered required check never reports, which leaves the pull
  // request pending rather than blocked.
  it("declares no path filter", () => {
    expect(triggers.pull_request.paths).toBeUndefined();
    expect(triggers.pull_request["paths-ignore"]).toBeUndefined();
  });

  it("targets any base branch", () => {
    expect(triggers.pull_request.branches).toBeUndefined();
    expect(triggers.pull_request["branches-ignore"]).toBeUndefined();
  });
});

describe("CIP-FR-03 concurrency", () => {
  it("groups per workflow and pull-request ref, cancelling in progress", () => {
    expect(workflow.concurrency.group).toContain("github.workflow");
    expect(workflow.concurrency.group).toContain("github.ref");
    expect(workflow.concurrency["cancel-in-progress"]).toBe(true);
  });

  // A run-unique key would give every run its own group, so cancel-in-progress
  // would never cancel anything while every other assertion still passed.
  it.each(["github.run_id", "github.run_number", "github.run_attempt", "github.sha"])(
    "does not key the group on %s",
    (context) => {
      expect(workflow.concurrency.group).not.toContain(context);
    },
  );
});

describe("CIP-FR-04 permissions", () => {
  it("grants read access to contents and nothing else", () => {
    expect(workflow.permissions).toEqual({ contents: "read" });
  });
});

describe("CIP-FR-05 lane independence", () => {
  it.each(LANES)("%s does not depend on another lane", (lane) => {
    expect(workflow.jobs[lane].needs).toBeUndefined();
  });

  // A matrix that holds two lanes would let one lane's failure cancel the
  // other, so both results would not always be reported. The areas of
  // ui-tests are one lane (CIP-FR-VRPM).
  it.each(LANES.filter((lane) => lane !== UI_TESTS))(
    "%s is a standalone job, not a matrix",
    (lane) => {
      expect(workflow.jobs[lane].strategy).toBeUndefined();
    },
  );

  it("ui-tests is a matrix over its own areas and no other dimension", () => {
    expect(Object.keys(workflow.jobs[UI_TESTS].strategy.matrix)).toEqual(["area"]);
  });
});

/**
 * The nastiest drift of all: continue-on-error makes a *failed* job report
 * `result: success` into `needs`, so a red lane produces a green gate and a
 * mergeable pull request. That is strictly worse than the pending-forever
 * failure the rest of this file guards, because it is invisible.
 */
describe("no continue-on-error anywhere", () => {
  it.each(ALL_JOBS)("%s does not swallow its own failure", (job) => {
    expect(workflow.jobs[job]["continue-on-error"]).toBeUndefined();
  });

  it.each(ALL_JOBS)("no step in %s swallows its failure", (job) => {
    for (const step of stepsOf(job)) {
      expect(step["continue-on-error"]).toBeUndefined();
    }
  });
});

describe("CIP-FR-06 UI lane provisioning", () => {
  it.each(UI_JOBS)("%s provisions pnpm before setup-node so the store cache resolves", (job) => {
    const refs = actionRefs(job);
    const pnpm = refs.findIndex((ref) => ref.startsWith("pnpm/action-setup"));
    const node = refs.findIndex((ref) => ref.startsWith("actions/setup-node"));
    expect(pnpm).toBeGreaterThanOrEqual(0);
    expect(node).toBeGreaterThan(pnpm);
  });

  // Compared as strings: quoting the scalar in YAML is equally valid and must
  // not flip this test.
  it.each(UI_JOBS)("%s pins the pnpm and Node major versions the project targets", (job) => {
    const steps = stepsOf(job);
    const pnpm = steps.find((s) => s.uses?.startsWith("pnpm/action-setup"));
    const node = steps.find((s) => s.uses?.startsWith("actions/setup-node"));
    expect(String(pnpm?.with?.version)).toBe("11");
    expect(String(node?.with?.["node-version"])).toBe("26");
  });

  it.each(UI_JOBS)("%s installs with a frozen lockfile", (job) => {
    expect(runScripts(job)).toContain("pnpm install --frozen-lockfile");
  });
});

describe("CIP-FR-07 UI lane checks", () => {
  it("runs the build and then the test typecheck", () => {
    const script = runScripts("ui");
    const install = script.indexOf("pnpm install --frozen-lockfile");
    const build = script.indexOf("pnpm build");
    const typecheck = script.indexOf(
      "pnpm exec tsc -p tsconfig.test.json --noEmit",
    );
    expect(build).toBeGreaterThan(install);
    expect(typecheck).toBeGreaterThan(build);
  });

  // The suite runs in the areas of ui-tests (CIP-FR-VRPM). A second full run
  // here would bring back the time limit that the areas remove.
  it("does not run the unit tests", () => {
    const script = runScripts("ui");
    expect(script).not.toContain("pnpm test");
    expect(script).not.toContain("vitest");
  });
});

describe("CIP-FR-08 UI lane caching", () => {
  it.each(UI_JOBS)("%s caches the pnpm store via setup-node", (job) => {
    const node = stepsOf(job).find((s) =>
      s.uses?.startsWith("actions/setup-node"),
    );
    expect(node?.with?.cache).toBe("pnpm");
  });
});

describe("CIP-FR-VRPM, CIP-FR-WKIN: the ui-tests areas", () => {
  const job = () => workflow.jobs[UI_TESTS];
  const testStep = () =>
    stepsOf(UI_TESTS).filter((s) => (s.run ?? "").includes("vitest"));

  // The same names, in the same order, as the Vitest projects of
  // CIP-FR-RDCY. A name that is not a project would run no test file.
  it("CIP-FR-VRPM, CIP-FR-RDCY: is a matrix of the three areas", () => {
    expect(job().strategy.matrix.area).toEqual([
      "editor and documents",
      "components",
      "app and state",
    ]);
    expect(job().strategy.matrix.area).toEqual(TEST_AREAS.map((a) => a.name));
  });

  // The area reaches the shell through env, so no expression goes into the
  // script (see "shell safety" below).
  it("CIP-FR-VRPM: each job runs its area after the install, as its only test step", () => {
    const steps = stepsOf(UI_TESTS);
    const install = steps.findIndex(
      (s) => (s.run ?? "").trim() === "pnpm install --frozen-lockfile",
    );
    expect(install).toBeGreaterThanOrEqual(0);
    const tests = testStep();
    expect(tests).toHaveLength(1);
    expect((tests[0].run ?? "").trim()).toBe('pnpm exec vitest run --project "$AREA"');
    expect(String(tests[0].env?.AREA)).toMatch(/^\$\{\{\s*matrix\.area\s*\}\}$/);
    expect(steps.indexOf(tests[0])).toBeGreaterThan(install);
    expect(runScripts(UI_TESTS)).not.toContain("pnpm test");
  });

  // The areas replace `pnpm test` in CI. If the script gets a flag that
  // changes which files run, the areas must get the same flag.
  it("CIP-FR-VRPM: the areas run the command of the pnpm test script", () => {
    const pkg = JSON.parse(readFileSync(resolve(ROOT, "package.json"), "utf8"));
    expect((testStep()[0]?.run ?? "").trim()).toBe(
      `pnpm exec ${pkg.scripts.test} --project "$AREA"`,
    );
  });

  it("CIP-FR-WKIN: a failing area does not cancel the other areas", () => {
    expect(job().strategy["fail-fast"]).toBe(false);
  });
});

describe("CIP-FR-09 backend system dependencies", () => {
  it.each([
    "libwebkit2gtk-4.1-dev",
    "libayatana-appindicator3-dev",
    "librsvg2-dev",
    "libxdo-dev",
    "libssl-dev",
    "build-essential",
    "curl",
    "wget",
    "file",
  ])("installs %s", (pkg) => {
    expect(runScripts("backend")).toContain(pkg);
  });

  it("installs them before any cargo invocation", () => {
    const script = runScripts("backend");
    expect(script.indexOf("apt-get install")).toBeLessThan(
      script.indexOf("cargo check"),
    );
  });
});

describe("CIP-FR-10 toolchain resolution", () => {
  // Read from the pinned file rather than hardcoded, so this keeps its
  // discriminating power after the channel is bumped — which is precisely the
  // scenario CIP-FR-10 describes.
  const channel = /channel\s*=\s*"([^"]+)"/.exec(
    readFileSync(resolve(ROOT, PIN), "utf8"),
  )?.[1];

  it("reads a channel from rust-toolchain.toml", () => {
    expect(channel).toBeTruthy();
  });

  /**
   * Grounded in the crates on disk rather than in lane ids: every crate the
   * repository tracks has to be verified by some Rust lane. That survives a
   * lane rename — which CIP-FR-16 says costs two edits to ci.yml and nothing
   * else — and still fires when a third crate is added with no lane to build
   * it, or when the discovery predicate above stops recognising a lane.
   */
  it("finds the crates the repository holds", () => {
    expect(CRATE_DIRS.length).toBeGreaterThanOrEqual(2);
  });

  it.each(CRATE_DIRS)("a Rust lane verifies %s", (crate) => {
    const lanes = RUST_LANES.filter((lane) => isInside(crate, workingDirOf(lane)));
    expect(lanes, `no Rust lane builds ${crate}`).not.toHaveLength(0);
  });

  // `rustup show` resolves the pin too, but installs only as a side effect of
  // rustup's auto-install setting: with that disabled it reports the toolchain
  // it did not install and the lane fails at the next cargo step instead,
  // pointing at the wrong step (CIP-FR-10). The argless install form is
  // unconditional and still names no version.
  it.each(RUST_LANES)(
    "%s installs the pinned toolchain unconditionally",
    (lane) => {
      expect(runScripts(lane)).toContain("rustup toolchain install");
    },
  );

  // Checked against what the workflow executes rather than its raw text: a
  // channel of `stable` or `nightly` would otherwise match any comment
  // containing the word, and fire on prose that changes nothing.
  it("never names the pinned channel in anything it executes", () => {
    for (const job of ALL_JOBS) {
      expect(runScripts(job)).not.toContain(channel!);
      for (const step of stepsOf(job)) {
        expect(JSON.stringify(step.with ?? {})).not.toContain(channel!);
        expect(JSON.stringify(step.env ?? {})).not.toContain(channel!);
      }
    }
  });

  it.each([
    ["a RUSTUP_TOOLCHAIN override", /RUSTUP_TOOLCHAIN/],
    ["a cargo +toolchain invocation", /cargo\s+\+/],
  ])("declares no %s", (_label, pattern) => {
    expect(source).not.toMatch(pattern);
  });

  // e.g. dtolnay/rust-toolchain@stable, actions-rs/toolchain, or
  // moonrepo/setup-rust — each of which silently supersedes the pin. Matched
  // by what they do rather than by one vendor's repository name.
  const TOOLCHAIN_INSTALLER = /toolchain|setup-rust|rust-setup/i;

  it.each(RUST_LANES)(
    "%s installs no toolchain via a third-party action",
    (lane) => {
      for (const ref of actionRefs(lane)) {
        expect(ref, `${ref} installs a toolchain the pin does not choose`).not.toMatch(
          TOOLCHAIN_INSTALLER,
        );
      }
    },
  );

  /**
   * rustup searches a lane's working directory and its parents for the pin and
   * nowhere else, which splits the lanes in two. A lane inside the pin's own
   * directory is governed by it for free; every other lane has to copy it in
   * first, and a lane that does not compiles with whatever toolchain the runner
   * defaults to — a green lane on the wrong compiler, which is exactly the
   * drift CIP-FR-10 exists to prevent.
   *
   * Partitioned rather than branched inside one test, so that neither half can
   * quietly empty out: the assertions below would all pass vacuously over an
   * empty list, and the two canaries are what stop that.
   */
  const PIN_DIR = posix.dirname(PIN);
  const [governed, mustCopy] = [
    RUST_LANES.filter((lane) => isInside(workingDirOf(lane), PIN_DIR)),
    RUST_LANES.filter((lane) => !isInside(workingDirOf(lane), PIN_DIR)),
  ];

  it("has a Rust lane on each side of the pin's directory", () => {
    expect(governed, `no Rust lane runs inside ${PIN_DIR}/`).not.toHaveLength(0);
    expect(mustCopy, `no Rust lane runs outside ${PIN_DIR}/`).not.toHaveLength(0);
  });

  it.each(governed)("the pin reaches %s without being copied", (lane) => {
    // rustup walks up from the working directory to the pin itself.
    expect(readFileSync(resolve(ROOT, PIN), "utf8")).toContain("[toolchain]");
    expect(isInside(workingDirOf(lane), PIN_DIR)).toBe(true);

    // And the lane does not plant a second pin of its own on the way.
    expect(
      pinCopyOf(lane),
      `${lane} already sits under ${PIN_DIR}/ yet copies a pin in`,
    ).toBeUndefined();
  });

  describe.each(mustCopy)("%s copies the pin in", (lane) => {
    const copy = pinCopyOf(lane);

    it("copies it at all", () => {
      expect(
        copy,
        `${lane} runs in ${workingDirOf(lane)}/, which the pin does not reach, and no step copies it there`,
      ).toBeDefined();
    });

    it("copies it into its own working directory", () => {
      // A destination left behind by a moved crate, or pointing at /tmp, leaves
      // rustup resolving the runner's default and the lane silently green.
      expect(copy?.destination).toBe(
        resolveFrom(workingDirOf(lane), PIN_FILE),
      );
    });

    it("copies it before anything the pin decides", () => {
      // By step index: a cache action runs no shell, but probes `rustc -vV`.
      expect(copy?.index ?? Number.POSITIVE_INFINITY).toBeLessThan(
        firstToolchainStep(lane),
      );
    });

    it("copies it unconditionally", () => {
      // A skipped copy leaves the ordering intact and the toolchain wrong.
      expect(copy?.step.if).toBeUndefined();
    });

    it("never commits the copy as a second pin", () => {
      // A committed copy would keep CI correct — CI overwrites it every run —
      // while local builds silently used a pin free to drift from the real one.
      // undefined means there was no git to ask, or the destination lies
      // outside the repository; the assertions above already cover both.
      expect(copy ? (tracked(copy.destination) ?? []) : []).toEqual([]);
    });
  });
});

describe("CIP-FR-11 ambient git config independence", () => {
  // The suite's fixtures set repository-local identity, so a bare runner is
  // what proves no test leans on a developer's global config. Configuring one
  // here would mask exactly the regression this guards.
  it("configures no global Git identity", () => {
    expect(runScripts("backend")).not.toMatch(
      /git\s+config\s+--global\s+user\.(name|email)/,
    );
  });
});

describe("CIP-FR-12 backend lane checks", () => {
  it("runs cargo check then cargo test", () => {
    const script = runScripts("backend");
    expect(script.indexOf("cargo check")).toBeGreaterThanOrEqual(0);
    expect(script.indexOf("cargo test")).toBeGreaterThan(
      script.indexOf("cargo check"),
    );
  });

  // Normalised, so rewriting the value as `./src-tauri` — semantically
  // identical to Actions — does not fail the suite as a behaviour change.
  it("runs them from src-tauri", () => {
    expect(workingDirOf("backend")).toBe("src-tauri");
  });
});

describe("CIP-FR-13 and CIP-FR-20 Rust lane caching", () => {
  const CACHED_LANES = RUST_LANES.filter((lane) => cacheStepOf(lane));

  /**
   * Grounded in the crates rather than in every Rust lane: FR-13 and FR-20
   * mandate a cache for the lanes that compile a crate, and demanding one from
   * every lane that merely touches the toolchain — a `cargo fmt --check` lane,
   * say — would make adding such a lane an edit to this file and break the
   * two-edit promise of CIP-FR-16.
   */
  it.each(CRATE_DIRS)("a cached Rust lane builds %s", (crate) => {
    const lanes = CACHED_LANES.filter((lane) => isInside(crate, workingDirOf(lane)));
    expect(lanes, `no cached Rust lane builds ${crate}`).not.toHaveLength(0);
  });

  // Its own, never shared: two lanes pointing at one workspace would collide
  // their caches, which is precisely what CIP-FR-13, CIP-FR-20 says must not happen.
  it.each(CACHED_LANES)("%s caches its own crate, not another lane's", (lane) => {
    expect(String(cacheStepOf(lane)?.with?.workspaces)).toBe(workingDirOf(lane));
  });

  /**
   * The action derives part of its key by probing `rustc -vV`, and a `uses:`
   * step ignores `defaults.run.working-directory` — it runs at the workspace
   * root, which the pin one directory down does not govern, so that probe
   * reports the runner's default toolchain rather than the pinned one. Without
   * the pin in the key, bumping the channel restores a target directory the
   * previous compiler built (CIP-FR-13, CIP-FR-20).
   */
  it.each(CACHED_LANES)("%s keys its cache on the pin", (lane) => {
    expect(String(cacheStepOf(lane)?.with?.key)).toContain(PIN);
  });
});

describe("CIP-FR-19 mock lane checks", () => {
  it("runs cargo check then cargo test from the mock crate", () => {
    const script = runScripts("mock");
    expect(script.indexOf("cargo check")).toBeGreaterThanOrEqual(0);
    expect(script.indexOf("cargo test")).toBeGreaterThan(
      script.indexOf("cargo check"),
    );
    expect(workingDirOf("mock")).toBe("tools/agentic-cli-mock");
  });

  // The committed lock file is the sole source of dependency versions; without
  // --locked a lock that disagrees with Cargo.toml is regenerated on the runner
  // instead of failing the lane.
  it.each(["cargo check", "cargo test"])("passes --locked to %s", (command) => {
    expect(runScripts("mock")).toContain(`${command} --locked`);
  });

  /**
   * ACM-FR-01 specifies a crate that links against nothing outside the Rust
   * toolchain. A lane that started installing the Tauri packages would mean the
   * crate had grown a dependency it is specified not to have, and the apt step
   * would hide that rather than surface it.
   */
  it.each(TAURI_PACKAGES)(
    "installs none of the backend lane's system packages (%s)",
    (pkg) => {
      expect(runScripts("mock")).not.toContain(pkg);
    },
  );
});

describe("CIP-FR-21 server lane checks", () => {
  it("runs cargo build then cargo test from the server crate", () => {
    const script = runScripts("server");
    expect(script.indexOf("cargo build")).toBeGreaterThanOrEqual(0);
    expect(script.indexOf("cargo test")).toBeGreaterThan(
      script.indexOf("cargo build"),
    );
    expect(workingDirOf("server")).toBe("server");
  });

  // `build`, not `check`: the crate's process-level tests start the binary, so
  // the lane has to produce it. A lane that only checked would compile nothing
  // to run, and the tests would build it themselves — outside the step whose
  // failure is meant to report a compile error (CIP-FR-21).
  it("builds the binary rather than only checking it", () => {
    expect(runScripts("server")).not.toContain("cargo check");
  });

  // The committed lock file is the sole source of dependency versions; without
  // --locked a lock that disagrees with Cargo.toml is regenerated on the runner
  // instead of failing the lane (CIP-FR-21, CIP-FR-22).
  it.each(["cargo build", "cargo test"])("passes --locked to %s", (command) => {
    expect(runScripts("server")).toContain(`${command} --locked`);
  });

  /**
   * BMS-FR-02 specifies a crate that links against nothing outside the Rust
   * toolchain, which is what CIP-FR-09, CIP-FR-21 checks on a bare runner.
   */
  it.each(TAURI_PACKAGES)(
    "installs none of the backend lane's system packages (%s)",
    (pkg) => {
      expect(runScripts("server")).not.toContain(pkg);
    },
  );
});

describe("CIP-FR-23 verification produces no image", () => {
  /**
   * CIP-FR-04, CIP-FR-23: the whole of the image's build and publication belongs to the
   * publishing workflow. A lane that started building or pushing one would need
   * a registry credential, which CIP-FR-04 denies it — so it would fail
   * confusingly at run time rather than here.
   */
  it.each(ALL_JOBS)("%s builds no image and logs in to no registry", (job) => {
    for (const ref of actionRefs(job)) {
      expect(ref).not.toMatch(/^docker\//);
    }
    const commands = commandsOf(runScripts(job));
    for (const command of ["docker", "buildx", "podman", "skopeo"]) {
      expect(commands).not.toContain(command);
    }
  });

  it("names no registry", () => {
    expect(source).not.toContain("ghcr.io");
  });
});

describe("CIP-FR-24 to CIP-FR-31 publishing workflow", () => {
  const PUBLISH = "publish";
  const publishJob = publishWorkflow.jobs?.[PUBLISH];
  const publishSteps: Step[] = publishJob?.steps ?? [];
  const publishScript = publishSteps.map((step) => step.run ?? "").join("\n");
  const stepUsing = (prefix: string): Step | undefined =>
    publishSteps.find((step) => step.uses?.startsWith(prefix));
  const buildStep = stepUsing("docker/build-push-action");
  const loginStep = stepUsing("docker/login-action");
  const withOf = (step: Step | undefined): Record<string, unknown> =>
    (step?.with ?? {}) as Record<string, unknown>;

  it("exists, is named Server image, and holds one job identified as publish", () => {
    expect(WORKFLOW_FILES).toContain(PUBLISH_FILE);
    expect(publishWorkflow.name).toBe("Server image");
    expect(Object.keys(publishWorkflow.jobs)).toEqual([PUBLISH]);
  });

  // CIP-FR-24, CIP-FR-25: the publication of a release, and nothing else. A
  // push trigger or a manual dispatch would let an image be published from
  // something that is not a release. `published` rather than `created` is what
  // makes a draft build when it is published rather than when it is saved.
  it("triggers on a published release and on nothing else", () => {
    const on = triggersOf(publishWorkflow);
    expect(Object.keys(on)).toEqual(["release"]);
    expect(on.release.types).toEqual(["published"]);
  });

  // CIP-FR-26: read-only at the workflow level, with the one write scope added
  // on the one job that needs it.
  it("grants contents: read at the workflow level and nothing more", () => {
    expect(publishWorkflow.permissions).toEqual({ contents: "read" });
  });

  it("adds packages: write on the publish job alone", () => {
    expect(publishJob.permissions).toEqual({
      contents: "read",
      packages: "write",
    });
  });

  // CIP-FR-26: the run's GITHUB_TOKEN and nothing else. A personal access token
  // or a registry password would be a long-lived credential in the repository.
  it("names the run's GITHUB_TOKEN as its only credential", () => {
    const referenced = [...publishSource.matchAll(/secrets\.(\w+)/g)].map(
      (match) => match[1],
    );
    expect(new Set(referenced)).toEqual(new Set(["GITHUB_TOKEN"]));

    expect(loginStep).toBeDefined();
    expect(withOf(loginStep).registry).toBe("ghcr.io");
    expect(withOf(loginStep).password).toBe("${{ secrets.GITHUB_TOKEN }}");
  });

  // CIP-FR-27: one manifest list holding both platforms, built from the crate's
  // own Dockerfile.
  it("builds both platforms from the server crate's Dockerfile and pushes", () => {
    expect(buildStep).toBeDefined();
    const options = withOf(buildStep);
    expect(options.platforms).toBe("linux/amd64,linux/arm64");
    expect(options.push).toBe(true);
    expect(String(options.file)).toContain("server/Dockerfile");
    expect(normaliseDir(String(options.context))).toBe("server");
  });

  /**
   * BMS-FR-21 reads the published tag as one manifest list holding exactly the
   * two platforms. Both attestations add an entry whose platform is
   * `unknown/unknown`, so leaving either on turns that reading into a manifest
   * list of four.
   */
  it("adds no attestation entry to the manifest list", () => {
    const options = withOf(buildStep);
    expect(options.provenance).toBe(false);
    expect(options.sbom).toBe(false);
  });

  // CIP-FR-28: the release's tag, and no second tag of any kind.
  it("pushes exactly one tag, and it is the release tag", () => {
    const tags = String(withOf(buildStep).tags)
      .split("\n")
      .map((tag) => tag.trim())
      .filter(Boolean);
    expect(tags).toHaveLength(1);

    // The reference is assembled by a shell step, so the tag reaches it through
    // that step's output rather than literally.
    expect(publishScript).toContain("${RELEASE_TAG}");
    expect(publishSource).toContain("github.event.release.tag_name");

    // No second tag of any kind: not `latest`, not a commit hash, not a branch.
    // Scoped to what actually names the image — the tags option and the script
    // that assembles the reference — because `runs-on: ubuntu-latest` puts the
    // word `latest` in every workflow file that ever ran on a hosted runner.
    const namesTheImage = [tags.join("\n"), publishScript].join("\n");
    expect(namesTheImage).not.toContain("latest");
    for (const forbidden of ["github.sha", "github.ref_name"]) {
      expect(publishSource).not.toContain(forbidden);
    }
  });

  // CIP-FR-27: a registry reference may not carry an upper-case character.
  it("folds the repository name to lower case", () => {
    expect(publishScript).toContain("${REPOSITORY,,}");
  });

  // CIP-FR-29: the published image reports the tag it was built from.
  it("passes the release tag into the build as SYNTHESIS_BUILD_VERSION", () => {
    const args = String(withOf(buildStep)["build-args"]);
    expect(args).toContain("SYNTHESIS_BUILD_VERSION=");
    expect(args).not.toContain("SYNTHESIS_BUILD_VERSION=\n");
  });

  /**
   * CIP-FR-30: a failure of any step fails the job. `continue-on-error` on the
   * push would report a green run for a release whose image never published,
   * which is exactly the silent absence the requirement exists to prevent.
   */
  it("lets no step swallow its own failure", () => {
    expect(publishJob["continue-on-error"]).toBeUndefined();
    for (const step of publishSteps) {
      expect(step["continue-on-error"]).toBeUndefined();
    }
  });

  // CIP-FR-31: no aggregate gate, so no check of this workflow can be named by
  // a branch-protection rule.
  it("declares no gate job", () => {
    expect(Object.keys(publishWorkflow.jobs)).not.toContain(GATE);
  });

  it("declares a plausible explicit timeout", () => {
    // CIP-FR-17. The ceiling is higher than a verification lane's: emulating
    // the second architecture makes this the slowest job in the repository.
    expect(publishJob["timeout-minutes"]).toBeGreaterThanOrEqual(5);
    expect(publishJob["timeout-minutes"]).toBeLessThanOrEqual(360);
  });

  it("pins every action to a version tag", () => {
    const refs = publishSteps
      .map((step) => step.uses)
      .filter((ref): ref is string => typeof ref === "string");
    expect(refs.length).toBeGreaterThan(0);
    for (const ref of refs) {
      expect(ref).toMatch(/@v\d+(\.\d+)*$/);
    }
  });

  // A release tag is attacker-influenced on a repository that accepts outside
  // maintainers; splicing it into a shell script is the standard injection
  // vector. It reaches the script through env: instead.
  it("interpolates no expression into a shell script", () => {
    for (const step of publishSteps) {
      expect(step.run ?? "").not.toContain("${{");
    }
  });
});

describe("CIP-FR-14 gate semantics", () => {
  it("depends on every lane", () => {
    expect([...workflow.jobs.gate.needs].sort()).toEqual([...LANES].sort());
  });

  // Without always(), a failing lane skips the gate, and a skipped required
  // check blocks the pull request as pending instead of failing it.
  it("runs regardless of lane outcomes", () => {
    expect(workflow.jobs.gate.if).toBe("always()");
  });

  it("fails on any non-success result", () => {
    const guard = gateGuard();
    expect(guard).toBeDefined();
    for (const result of ["failure", "cancelled", "skipped"]) {
      expect(guard?.if).toContain(`'${result}'`);
    }
  });
});

describe("CIP-FR-15 check names", () => {
  it("exposes the gate alongside at least one lane", () => {
    expect(ALL_JOBS).toContain(GATE);
    expect(LANES.length).toBeGreaterThan(0);
  });

  // An overridden display name would change the check name branch protection
  // is configured against.
  it.each(ALL_JOBS)("%s does not override its display name", (job) => {
    expect(workflow.jobs[job].name).toBeUndefined();
  });

  // The job ids, together with no display name above and the areas of
  // CIP-FR-VRPM, give one check name CI / ui-tests (<area>) for each area.
  it("has the two frontend lanes", () => {
    expect(LANES).toEqual(expect.arrayContaining(UI_JOBS));
  });
});

describe("CIP-FR-16 extensibility", () => {
  // Hardcoding lane ids in the guard would make adding a lane a three-edit
  // change and silently leave the new lane non-blocking.
  it("guards the gate generically over all dependencies", () => {
    const guard = gateGuard();
    expect(guard?.if).toContain("needs.*.result");
    for (const lane of LANES) {
      expect(guard?.if).not.toContain(`needs.${lane}.result`);
    }
  });
});

describe("CIP-FR-17 timeouts", () => {
  // A floor as well as a ceiling: timeout-minutes: 1 would kill every run
  // while still satisfying a bare "is set" assertion.
  it.each(ALL_JOBS)("%s declares a plausible explicit timeout", (job) => {
    const timeout = workflow.jobs[job]["timeout-minutes"];
    expect(timeout).toBeGreaterThanOrEqual(5);
    expect(timeout).toBeLessThanOrEqual(60);
  });
});

describe("CIP-FR-18 secretless", () => {
  it.each([
    ["dot access", /\bsecrets\s*\./],
    ["index access", /\bsecrets\s*\[/],
    ["whole-context serialisation", /toJSON\(\s*secrets\s*\)/],
  ])("reads no repository secret by %s", (_label, pattern) => {
    expect(source).not.toMatch(pattern);
  });

  it("requests no write-scoped permission", () => {
    for (const scope of Object.values(
      workflow.permissions as Record<string, string>,
    )) {
      expect(scope).toBe("read");
    }
  });
});

describe("shell safety", () => {
  // The needs and github contexts carry attacker-influenced values; splicing
  // them into a run: script is the standard Actions injection vector.
  it.each(ALL_JOBS)("%s interpolates no expression into a shell script", (job) => {
    for (const step of stepsOf(job)) {
      expect(step.run ?? "").not.toContain("${{");
    }
  });
});

describe("action pinning", () => {
  // A mutable ref would let a third party change what CI executes.
  const jobsWithActions = ALL_JOBS.filter((job) => actionRefs(job).length > 0);

  it("has jobs that use actions", () => {
    expect(jobsWithActions.length).toBeGreaterThan(0);
  });

  it.each(jobsWithActions)("%s pins every action to a version tag", (job) => {
    for (const ref of actionRefs(job)) {
      expect(ref).toMatch(/@v\d+(\.\d+)*$/);
    }
  });
});
