/**
 * The detail read of the issue view, as a unit
 * (`../../../specifications/ui/MIV-map-issue-view.md` MIV-FR-DFVC,
 * MIV-FR-ZQAX, MIV-FR-IHRB).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

const read = vi.hoisted(() => vi.fn());
vi.mock("../../api", async (original) => ({
  ...(await original<typeof import("../../api")>()),
  readGithubPlanIssue: read,
}));
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

import { logInfo, logWarn } from "../../logging";
import { issueRead } from "../../test/deliveryLensHarness";
import { siblingIssueUrl, useGithubIssueDetail, type DetailTarget } from "./IssueDetailFields";

afterEach(() => {
  cleanup();
  read.mockReset();
  vi.mocked(logWarn).mockClear();
  vi.mocked(logInfo).mockClear();
});

describe("siblingIssueUrl (MIV-FR-ZQAX)", () => {
  it.each([
    ["https://github.com/acme/payments/issues/41", "https://github.com/acme/payments/issues/77"],
    ["https://github.com/acme/payments/issues/41/", "https://github.com/acme/payments/issues/77/"],
    ["https://github.com/acme/payments/issues/41?x=1", "https://github.com/acme/payments/issues/77"],
    ["https://ghe.example.com/acme/payments/milestone/3", "https://ghe.example.com/acme/payments/issues/77"],
  ])("MIV-FR-ZQAX: the sub-issue of %s is %s", (from, to) => {
    expect(siblingIssueUrl(from, 77, "acme/payments")).toBe(to);
  });
});

describe("useGithubIssueDetail (MIV-FR-DFVC, MIV-FR-IHRB)", () => {
  const target = (number: number, updatedAt = "a"): DetailTarget => ({ itemId: `i-${number}`, number, updatedAt });

  it("MIV-FR-DFVC: no target reads nothing; a target change discards the answer of the earlier one", async () => {
    const pending: Array<(v: unknown) => void> = [];
    read.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));
    const { result, rerender } = renderHook(({ t }: { t: DetailTarget | null }) => useGithubIssueDetail(t), {
      initialProps: { t: null as DetailTarget | null },
    });
    expect(result.current).toBeNull();
    expect(read).not.toHaveBeenCalled();
    rerender({ t: target(41) });
    expect(result.current).toEqual({ itemId: "i-41", status: "loading" });
    rerender({ t: target(42) });
    await act(async () => pending[1](issueRead(42, { labels: ["new"] })));
    await act(async () => pending[0](issueRead(41, { labels: ["old"] })));
    expect(result.current).toMatchObject({ itemId: "i-42", status: "ready" });
    expect(result.current?.status === "ready" && result.current.read.issue.labels).toEqual(["new"]);
  });

  it("MIV-FR-BCOP, MIV-FR-IHRB: a refusal gives the failed state and one warning that holds the error code alone", async () => {
    read.mockRejectedValue({ code: "tracker_item_not_found", message: "Issue 41 Card capture is not in the plan." });
    const { result } = renderHook(() => useGithubIssueDetail(target(41)));
    await act(async () => {});
    expect(result.current).toEqual({ itemId: "i-41", status: "failed" });
    expect(logWarn).toHaveBeenCalledTimes(1);
    expect(vi.mocked(logWarn).mock.calls[0][2]).toEqual({ code: "tracker_item_not_found" });
    expect(JSON.stringify(vi.mocked(logWarn).mock.calls)).not.toMatch(/Card capture/);
  });

  it("MIV-FR-IHRB: an answer logs its outcome with no title, body, or label", async () => {
    read.mockResolvedValue(issueRead(41, { title: "Card capture", body: "secret body", labels: ["p1"] }));
    renderHook(() => useGithubIssueDetail(target(41)));
    await act(async () => {});
    const fields = vi.mocked(logInfo).mock.calls.map((c) => JSON.stringify(c));
    expect(fields.some((f) => f.includes("issue detail read"))).toBe(true);
    expect(fields.join()).not.toMatch(/Card capture|secret body|p1/);
  });
});
