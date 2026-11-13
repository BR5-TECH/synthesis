/**
 * A refused act of the Runs section that is an OpenSpec failure
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md` OFN-FR-LKAT,
 * OFN-FR-QZEC; `../../../specifications/ui/GRU-graduation-runs.md`
 * GRU-FR-XQVG).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { flushLogs, resetLogBufferForTest } from "../../logging";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
/** The rejection of a command, where the test gives one. */
let refusals: Record<string, unknown>;

beforeEach(() => {
  refusals = {};
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command in refusals) throw refusals[command];
    switch (command) {
      case "list_graduation_queue":
        return makeQueue([makeRun("r1", "working")]);
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      default:
        return undefined;
    }
  });
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
  resetLogBufferForTest();
});
afterEach(cleanup);

/** The OFN-FR-LKAT records sent so far. */
function noticeRecords() {
  flushLogs();
  return invoked.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: Array<{ message: string; fields: unknown }> }).records)
    .filter((r) => r.message === "an openspec failure notice appeared");
}

describe("a refused act (GRU-FR-XQVG)", () => {
  it("OFN-FR-LKAT, OFN-FR-QZEC: an act refused with an OpenSpec failure states it once, and emits one WARN record", async () => {
    render(<GraduationRuns />);
    await screen.findByTestId("graduation-section");
    refusals = { pause_graduation_run: { code: "cli_timed_out", diagnostics: [{ message: "secret" }] } };
    await userEvent.click(await screen.findByRole("button", { name: "Pause “Run r1”" }));
    const region = screen.getByRole("region", { name: "The selected run" });
    await waitFor(() =>
      expect(region).toHaveTextContent("The OpenSpec CLI did not finish in time, also after one more try."),
    );
    expect(within(region).getAllByText(/did not finish in time/)).toHaveLength(1);
    const records = noticeRecords();
    expect(records).toHaveLength(1);
    expect(records[0].fields).toMatchObject({ code: "cli_timed_out", class: "transient" });
    expect(JSON.stringify(records)).not.toContain("secret");
  });

  it("OFN-FR-LKAT: an act refused with a code that is not an OpenSpec failure emits no notice record", async () => {
    render(<GraduationRuns />);
    await screen.findByTestId("graduation-section");
    refusals = { pause_graduation_run: "run_not_working" };
    await userEvent.click(await screen.findByRole("button", { name: "Pause “Run r1”" }));
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(await within(region).findByTestId("graduation-error")).toBeInTheDocument();
    expect(noticeRecords()).toHaveLength(0);
  });
});
