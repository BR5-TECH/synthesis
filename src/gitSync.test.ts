/**
 * Whether Push is offered (CHG-FR-37), over more than one repository
 * (CHG-FR-KQWD).
 */
import { describe, expect, it } from "vitest";

import { canPush, pushUnavailableReason } from "./gitSync";
import type { RepositorySyncState, UpstreamSyncState } from "./types";

const member = (over: Partial<RepositorySyncState>): RepositorySyncState => ({
  repository: "",
  branch: "main",
  hasRemote: true,
  hasUpstream: true,
  ahead: 0,
  behind: 0,
  ...over,
});

const state = (repositories: RepositorySyncState[]): UpstreamSyncState => ({
  hasRemote: true,
  hasUpstream: true,
  ahead: 0,
  behind: 0,
  repositories,
});

describe("canPush with more than one repository (CHG-FR-KQWD)", () => {
  it("CHG-FR-KQWD: a submodule on a branch with commits to publish makes Push available", () => {
    const s = state([member({}), member({ repository: "vendor/kit", ahead: 2 })]);
    expect(canPush(s)).toBe(true);
    expect(pushUnavailableReason(s)).toBeNull();
  });

  it("CHG-FR-KQWD, CHG-FR-37: a submodule with a remote and no upstream makes Push available", () => {
    const s = state([
      member({}),
      member({ repository: "vendor/kit", hasUpstream: false, ahead: null, behind: null }),
    ]);
    expect(canPush(s)).toBe(true);
  });

  it("CHG-FR-KQWD: a detached submodule never makes Push available", () => {
    const s = state([
      member({}),
      member({ repository: "vendor/kit", branch: undefined, ahead: 5 }),
    ]);
    expect(canPush(s)).toBe(false);
    expect(pushUnavailableReason(s)).toContain("no repository on a branch");
  });

  it("CHG-FR-KQWD: the main repository on a detached HEAD does not make Push available", () => {
    const s = state([
      member({ branch: undefined, ahead: 3 }),
      member({ repository: "vendor/kit" }),
    ]);
    expect(canPush(s)).toBe(false);
  });

  it("CHG-FR-KQWD, CHG-FR-37: no repository with a remote is not pushable, and says so", () => {
    const s = state([
      member({ hasRemote: false, hasUpstream: false, ahead: null }),
      member({ repository: "vendor/kit", hasRemote: false, hasUpstream: false, ahead: null }),
    ]);
    expect(canPush(s)).toBe(false);
    expect(pushUnavailableReason(s)).toBe(
      "No repository of this project has a remote configured.",
    );
  });

  it("CHG-FR-37: one listed repository keeps the rule on the top-level fields", () => {
    const s: UpstreamSyncState = {
      ...state([member({ ahead: 0 })]),
      ahead: 1,
    };
    expect(canPush(s)).toBe(true);
  });
});
