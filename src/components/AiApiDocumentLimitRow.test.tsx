/**
 * The AI API section's Document size limit
 * (`../../specifications/ui/GLS-global-settings.md` GLS-FR-DLMP, GLS-FR-WQHT,
 * GLS-FR-16).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../logging", async (original) => ({
  ...(await original<typeof import("../logging")>()),
  logWarn: vi.fn(),
}));

import { AiApiDocumentLimitRow, documentLimitToCommit } from "./AiApiDocumentLimitRow";
import { logWarn, resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import {
  BothLevels,
  apiLevel,
  backendFor,
  commandsFor,
} from "../test/aiIntegrationsFixtures";

const commands = commandsFor(invokeMock);
const SAVE = "save_document_limit_percent";
const saves = () => invokeMock.mock.calls.filter((c) => c[0] === SAVE);

/** The stored percent, and how a save answers. */
let stored = 30;
let refuse: string | null = null;

function routed(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  if (cmd === "load_document_limit_percent") return Promise.resolve(stored);
  if (cmd === SAVE) {
    if (refuse !== null) return Promise.reject(refuse);
    stored = args!.percent as number;
    return Promise.resolve(stored);
  }
  return otherCommand(cmd);
}

async function field() {
  const input = (await screen.findByLabelText("Document size limit")) as HTMLInputElement;
  await waitFor(() => expect(input).toHaveValue("30"));
  return input;
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
    routed(cmd, args),
  );
  vi.mocked(logWarn).mockClear();
  stored = 30;
  refuse = null;
});

afterEach(cleanup);

describe("documentLimitToCommit", () => {
  it.each([
    ["", undefined],
    ["0", undefined],
    ["1", 1],
    [" 30 ", 30],
    ["100", 100],
    ["101", undefined],
    ["2.5", undefined],
    ["-5", undefined],
    ["abc", undefined],
  ])("GLS-FR-WQHT: %j commits %j", (text, expected) => {
    expect(documentLimitToCommit(text)).toBe(expected);
  });
});

describe("the Document size limit field", () => {
  it("GLS-FR-DLMP: reads the stored percent on mount and says what it is", async () => {
    render(<AiApiDocumentLimitRow />);
    await field();
    expect(invokeMock).toHaveBeenCalledWith("load_document_limit_percent");
    expect(screen.getByTestId("ai-api-document-limit-note")).toHaveTextContent(
      "How much of the model's context window the document under discussion may fill.",
    );
  });

  it("GLS-FR-WQHT: commits on Enter and applies at once", async () => {
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "45{Enter}");
    await waitFor(() => expect(saves()).toEqual([[SAVE, { percent: 45 }]]));
    expect(input).toHaveValue("45");
  });

  it("GLS-FR-WQHT: commits on blur, and Enter then blur sends one call", async () => {
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "20{Enter}");
    await user.tab();
    await waitFor(() => expect(input).toHaveValue("20"));
    expect(saves()).toHaveLength(1);
  });

  it("GLS-FR-WQHT: commits on blur alone, with no Enter", async () => {
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "20");
    expect(saves()).toHaveLength(0);
    await user.tab();
    await waitFor(() => expect(saves()).toEqual([[SAVE, { percent: 20 }]]));
    expect(input).toHaveValue("20");
  });

  it("GLS-FR-WQHT: a refusal for another reason restores the stored value and says the limit was not saved", async () => {
    refuse = "store_unavailable";
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "50{Enter}");
    expect(await screen.findByTestId("ai-api-document-limit-error")).toHaveTextContent(
      "The limit could not be saved.",
    );
    expect(input).toHaveValue("30");
  });

  it.each(["0", "101", "abc", ""])(
    "GLS-FR-WQHT: %j shows an inline message and commits nothing",
    async (text) => {
      const user = userEvent.setup();
      render(<AiApiDocumentLimitRow />);
      const input = await field();
      await user.clear(input);
      if (text !== "") await user.type(input, text);
      expect(screen.getByTestId("ai-api-document-limit-invalid")).toHaveTextContent(
        "Enter a whole number from 1 to 100.",
      );
      expect(input).toHaveAttribute("aria-invalid", "true");
      await user.type(input, "{Enter}");
      await user.tab();
      expect(saves()).toHaveLength(0);
      // The blur returns the field to the stored value.
      expect(input).toHaveValue("30");
    },
  );

  it("GLS-FR-WQHT: a refused save restores the stored value and shows the refusal", async () => {
    refuse = "percent_out_of_range";
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "50{Enter}");
    expect(await screen.findByTestId("ai-api-document-limit-error")).toHaveTextContent(
      "Enter a whole number from 1 to 100.",
    );
    expect(input).toHaveValue("30");
    expect(logWarn).toHaveBeenCalled();
  });

  it("GLS-FR-WQHT: the same value commits nothing", async () => {
    const user = userEvent.setup();
    render(<AiApiDocumentLimitRow />);
    const input = await field();
    await user.type(input, "{Enter}");
    await user.tab();
    expect(saves()).toHaveLength(0);
  });
});

describe("its place in the AI API section", () => {
  it("GLS-FR-DLMP, GLS-FR-16: stands above the provider tabs, once for every provider", async () => {
    backendFor(invokeMock)();
    render(<BothLevels />);
    const level = apiLevel();
    const row = await level.findByTestId("ai-api-document-limit-row");
    const tabs = level.getByRole("tablist", { name: "AI API" });
    expect(row.compareDocumentPosition(tabs) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(level.getByRole("tablist", { name: "AI API" })).queryByTestId(
      "ai-api-document-limit-row",
    )).toBeNull();
    expect(level.getAllByTestId("ai-api-document-limit-row")).toHaveLength(1);
    await waitFor(() =>
      expect(commands()).toContain("load_document_limit_percent"),
    );
  });
});
