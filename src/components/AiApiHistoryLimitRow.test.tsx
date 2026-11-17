/**
 * The AI API section's History size limit
 * (`../../specifications/ui/GLS-global-settings.md` GLS-FR-UTNL, GLS-FR-QHNI).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../logging", async (original) => ({
  ...(await original<typeof import("../logging")>()),
  logWarn: vi.fn(),
}));

import { AiApiHistoryLimitRow } from "./AiApiHistoryLimitRow";
import { logWarn, resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import { BothLevels, apiLevel, backendFor, commandsFor } from "../test/aiIntegrationsFixtures";

const commands = commandsFor(invokeMock);
const SAVE = "save_history_limit_percent";
const saves = () => invokeMock.mock.calls.filter((c) => c[0] === SAVE);

/** The stored percent, and how a save answers. */
let stored = 20;
let refuse: string | null = null;

function routed(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  if (cmd === "load_history_limit_percent") return Promise.resolve(stored);
  if (cmd === SAVE) {
    if (refuse !== null) return Promise.reject(refuse);
    stored = args!.percent as number;
    return Promise.resolve(stored);
  }
  return otherCommand(cmd);
}

async function field() {
  const input = (await screen.findByLabelText("History size limit")) as HTMLInputElement;
  await waitFor(() => expect(input).toHaveValue("20"));
  return input;
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
    routed(cmd, args),
  );
  vi.mocked(logWarn).mockClear();
  stored = 20;
  refuse = null;
});

afterEach(() => {
  cleanup();
  resetLogBufferForTest();
});

describe("the History size limit field", () => {
  it("GLS-FR-UTNL: reads the stored percent on mount and says what it is", async () => {
    // A value that is not the default, so the field shows what it read.
    stored = 37;
    render(<AiApiHistoryLimitRow />);
    const input = (await screen.findByLabelText("History size limit")) as HTMLInputElement;
    await waitFor(() => expect(input).toHaveValue("37"));
    expect(invokeMock).toHaveBeenCalledWith("load_history_limit_percent");
    expect(screen.getByTestId("ai-api-history-limit-note")).toHaveTextContent(
      "How much of the model's context window the discussion history may fill.",
    );
  });

  it("GLS-FR-QHNI: commits on Enter and applies at once", async () => {
    const user = userEvent.setup();
    render(<AiApiHistoryLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "45{Enter}");
    await waitFor(() => expect(saves()).toEqual([[SAVE, { percent: 45 }]]));
    expect(input).toHaveValue("45");
  });

  it("GLS-FR-QHNI: commits on blur, and Enter then blur sends one call", async () => {
    const user = userEvent.setup();
    render(<AiApiHistoryLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "35{Enter}");
    await user.tab();
    await waitFor(() => expect(input).toHaveValue("35"));
    expect(saves()).toHaveLength(1);
  });

  it("GLS-FR-QHNI: commits on blur alone, with no Enter", async () => {
    const user = userEvent.setup();
    render(<AiApiHistoryLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "35");
    expect(saves()).toHaveLength(0);
    await user.tab();
    await waitFor(() => expect(saves()).toEqual([[SAVE, { percent: 35 }]]));
    expect(input).toHaveValue("35");
  });

  it.each(["0", "101", "abc", ""])(
    "GLS-FR-QHNI: %j shows an inline message and commits nothing",
    async (text) => {
      const user = userEvent.setup();
      render(<AiApiHistoryLimitRow />);
      const input = await field();
      await user.clear(input);
      if (text !== "") await user.type(input, text);
      expect(screen.getByTestId("ai-api-history-limit-invalid")).toHaveTextContent(
        "Enter a whole number from 1 to 100.",
      );
      expect(input).toHaveAttribute("aria-invalid", "true");
      await user.type(input, "{Enter}");
      await user.tab();
      expect(saves()).toHaveLength(0);
      expect(input).toHaveValue("20");
    },
  );

  it("GLS-FR-QHNI: a refused save restores the stored value and shows the refusal", async () => {
    refuse = "percent_out_of_range";
    const user = userEvent.setup();
    render(<AiApiHistoryLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "50{Enter}");
    expect(await screen.findByTestId("ai-api-history-limit-error")).toHaveTextContent(
      "Enter a whole number from 1 to 100.",
    );
    expect(input).toHaveValue("20");
    expect(logWarn).toHaveBeenCalledWith(
      ["frontend", "ai"],
      "the history size limit was refused",
      expect.anything(),
    );
  });
});

describe("its place in the AI API section", () => {
  it("GLS-FR-UTNL: stands below the Document size limit, once for every provider", async () => {
    backendFor(invokeMock)();
    render(<BothLevels />);
    const level = apiLevel();
    const history = await level.findByTestId("ai-api-history-limit-row");
    const document = level.getByTestId("ai-api-document-limit-row");
    const tabs = level.getByRole("tablist", { name: "AI API" });
    expect(document.compareDocumentPosition(history) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(history.compareDocumentPosition(tabs) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(level.getAllByTestId("ai-api-history-limit-row")).toHaveLength(1);
    await waitFor(() => expect(commands()).toContain("load_history_limit_percent"));
  });
});
