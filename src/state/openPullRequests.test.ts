/**
 * The open pull requests that the openers of the Create a PR window share
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-RWOE,
 * CPR-FR-TJYF).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import { pullRequest } from "../test/gitPanelFixtures";
import type { PullRequestSummary } from "../types";
import {
  clearOpenPullRequests,
  openPullRequestFor,
  openPullRequestReason,
  refreshOpenPullRequests,
  resetOpenPullRequestsForTest,
  useOpenPullRequestList,
} from "./openPullRequests";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** The reads of `list_pull_requests`, each settled by the test. */
let reads: { resolve: (rows: PullRequestSummary[]) => void; reject: (e: unknown) => void }[];

beforeEach(() => {
  resetLogBufferForTest();
  resetOpenPullRequestsForTest();
  reads = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "list_pull_requests") {
      return new Promise((resolve, reject) => reads.push({ resolve, reject }));
    }
    return Promise.reject(new Error(`unexpected ${cmd}`));
  });
});

const listCalls = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "list_pull_requests");

describe("the open pull requests of the openers (CPR-FR-RWOE, CPR-FR-TJYF)", () => {
  it("CPR-FR-RWOE: reads the open pull requests, and names the one whose head is the branch", async () => {
    const { result } = renderHook(() => useOpenPullRequestList("feature"));
    expect(listCalls()[0][1]).toEqual({ state: "open" });
    expect(result.current).toBeNull();
    await act(async () => reads[0].resolve([pullRequest(4, { headBranch: "feature" })]));
    expect(openPullRequestFor(result.current, "feature")?.number).toBe(4);
    expect(openPullRequestFor(result.current, "other")).toBeNull();
    expect(openPullRequestReason(pullRequest(4))).toBe(
      "Pull request #4 is already open for this branch.",
    );
  });

  it("CPR-FR-RWOE: a closed or merged pull request names nothing", () => {
    const rows = [
      pullRequest(1, { headBranch: "feature", state: "closed" }),
      pullRequest(2, { headBranch: "feature", state: "merged" }),
    ];
    expect(openPullRequestFor(rows, "feature")).toBeNull();
  });

  it("CPR-FR-RWOE: an open pull request from a fork with the same branch name names nothing", () => {
    const rows = [pullRequest(3, { headBranch: "fix-typo", headIsFork: true })];
    expect(openPullRequestFor(rows, "fix-typo")).toBeNull();
    const both = [...rows, pullRequest(5, { headBranch: "fix-typo" })];
    expect(openPullRequestFor(both, "fix-typo")?.number).toBe(5);
  });

  it("CPR-FR-RWOE: a failed read names nothing", async () => {
    const { result } = renderHook(() => useOpenPullRequestList("feature"));
    await act(async () => reads[0].reject("github_unreachable"));
    expect(result.current).toBeNull();
    expect(openPullRequestFor(result.current, "feature")).toBeNull();
  });

  it("CPR-FR-RWOE: callers that ask while a read runs share it", () => {
    renderHook(() => useOpenPullRequestList("feature"));
    renderHook(() => useOpenPullRequestList("other"));
    void refreshOpenPullRequests();
    expect(listCalls()).toHaveLength(1);
  });

  it("CPR-FR-TJYF: a read after a creation waits for the running read and then reads again", async () => {
    void refreshOpenPullRequests();
    const fresh = refreshOpenPullRequests(true);
    expect(listCalls()).toHaveLength(1);
    await act(async () => reads[0].resolve([]));
    await waitFor(() => expect(listCalls()).toHaveLength(2));
    await act(async () => reads[1].resolve([pullRequest(9, { headBranch: "feature" })]));
    await fresh;
    const { result } = renderHook(() => useOpenPullRequestList(null, false));
    expect(openPullRequestFor(result.current, "feature")?.number).toBe(9);
  });

  it("a change of the project forgets the list, and a stale read changes nothing", async () => {
    const { result } = renderHook(() => useOpenPullRequestList("feature"));
    act(() => clearOpenPullRequests());
    await act(async () => reads[0].resolve([pullRequest(4, { headBranch: "feature" })]));
    expect(result.current).toBeNull();
  });
});
