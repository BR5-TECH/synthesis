/**
 * A dev container that could not start, or that lacks the agent CLI, as the run
 * and its raise name it (GRU-FR-PTFJ, NTF-FR-TFIY, GXD-FR-ZPYN), and the start
 * refusals of a dev container project (GSU-FR-SQRC).
 */
import { describe, expect, it } from "vitest";

import { makeMergeRun, makeRun } from "../../test/graduationFixtures";
import { devcontainerFailureCause, raiseLevel, raiseStatement, sandboxFailureCause } from "./escalations";
import { graduationErrorMessage } from "./messages";

const START_DETAIL =
  "The dev container could not start. Correct the cause, or choose Vendor images in Project settings, in the Docker section. Then use Continue. The dev container CLI reported: Error: image not found";
const CLI_DETAIL =
  "The dev container does not hold the agent CLI `claude`. Add it in Project settings, in the Docker section, and build the dev container again. Then use Continue.";

const stopped = (detail: string) =>
  makeRun("r1", "interrupted", {
    interruption: { reason: "launch_failed", detail, at: "2026-11-21T10:00:00Z" },
  });

describe("a dev container launch failure (GRU-FR-PTFJ, NTF-FR-TFIY)", () => {
  it("GRU-FR-PTFJ, NTF-FR-TFIY: a start failure raises at Error and names the dev container", () => {
    const run = stopped(START_DETAIL);
    expect(devcontainerFailureCause(run)).toBe("the dev container could not start");
    expect(sandboxFailureCause(run)).toBeNull();
    expect(raiseLevel(run)).toBe("Error");
    expect(raiseStatement(run)).toMatch(/stopped: the dev container could not start\.$/);
  });

  it("GRU-FR-PTFJ, NTF-FR-TFIY: a missing CLI names the CLI, also for a merge run", () => {
    expect(devcontainerFailureCause(stopped(CLI_DETAIL))).toBe(
      "the dev container does not hold the agent CLI claude",
    );
    const merge = makeMergeRun("m1", "interrupted", {
      interruption: { reason: "launch_failed", detail: CLI_DETAIL, at: "2026-11-21T10:00:00Z" },
    });
    expect(raiseLevel(merge)).toBe("Error");
    expect(raiseStatement(merge)).toMatch(/does not hold the agent CLI claude\.$/);
  });

  it("GRU-FR-PTFJ: another launch failure is not a dev container failure", () => {
    expect(devcontainerFailureCause(stopped("Something else."))).toBeNull();
    const other = makeRun("r2", "interrupted", {
      interruption: { reason: "agent_exited", detail: START_DETAIL, at: "2026-11-21T10:00:00Z" },
    });
    expect(devcontainerFailureCause(other)).toBeNull();
  });
});

describe("the start refusals of a dev container (GSU-FR-SQRC)", () => {
  it("GSU-FR-SQRC: each of the five refusals names what to configure", () => {
    expect(graduationErrorMessage("devcontainer_config_missing")).toMatch(/selects no devcontainer.json/);
    expect(graduationErrorMessage("devcontainer_config_invalid")).toMatch(/not valid JSON with comments/);
    expect(graduationErrorMessage("devcontainer_cli_missing")).toMatch(/Global settings, in the Docker section/);
    expect(graduationErrorMessage("docker_cli_required")).toMatch(/No Docker CLI was found/);
    expect(graduationErrorMessage("Error: devcontainer_agent_cli_missing")).toMatch(/does not hold the active agent's CLI/);
  });
});
