/**
 * The AI API section's Extracted text limit
 * (`../../specifications/ui/GLS-global-settings.md` GLS-FR-GLFQ, GLS-FR-LZLG).
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

import { AiApiExtractedTextLimitRow } from "./AiApiExtractedTextLimitRow";
import { logWarn, resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import { BothLevels, apiLevel, backendFor, commandsFor } from "../test/aiIntegrationsFixtures";

const commands = commandsFor(invokeMock);
const SAVE = "save_extracted_text_limit_kb";
const saves = () => invokeMock.mock.calls.filter((c) => c[0] === SAVE);

/** The stored kilobytes, and how a save answers. */
let stored = 1024;
let refuse: string | null = null;

function routed(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  if (cmd === "load_extracted_text_limit_kb") return Promise.resolve(stored);
  if (cmd === SAVE) {
    if (refuse !== null) return Promise.reject(refuse);
    stored = args!.kb as number;
    return Promise.resolve(stored);
  }
  return otherCommand(cmd);
}

async function field() {
  const input = (await screen.findByLabelText("Extracted text limit")) as HTMLInputElement;
  await waitFor(() => expect(input).toHaveValue("1024"));
  return input;
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
    routed(cmd, args),
  );
  vi.mocked(logWarn).mockClear();
  stored = 1024;
  refuse = null;
});

afterEach(() => {
  cleanup();
  resetLogBufferForTest();
});

describe("the Extracted text limit field", () => {
  it("GLS-FR-GLFQ: reads the stored kilobytes on mount and says what it is", async () => {
    stored = 300;
    render(<AiApiExtractedTextLimitRow />);
    const input = (await screen.findByLabelText("Extracted text limit")) as HTMLInputElement;
    await waitFor(() => expect(input).toHaveValue("300"));
    expect(invokeMock).toHaveBeenCalledWith("load_extracted_text_limit_kb");
    expect(screen.getByTestId("ai-api-extracted-text-limit-note")).toHaveTextContent(
      "The most text that one attached PDF or DOCX file may give.",
    );
    expect(screen.getByTestId("ai-api-extracted-text-limit-row")).toHaveTextContent("KB");
  });

  it("GLS-FR-LZLG: commits on Enter and applies at once", async () => {
    const user = userEvent.setup();
    render(<AiApiExtractedTextLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "2048{Enter}");
    await waitFor(() => expect(saves()).toEqual([[SAVE, { kb: 2048 }]]));
    expect(input).toHaveValue("2048");
  });

  it("GLS-FR-LZLG: commits on blur, and Enter then blur sends one call", async () => {
    const user = userEvent.setup();
    render(<AiApiExtractedTextLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "512{Enter}");
    await user.tab();
    await waitFor(() => expect(input).toHaveValue("512"));
    expect(saves()).toHaveLength(1);
  });

  it("GLS-FR-LZLG: commits on blur alone, with no Enter", async () => {
    const user = userEvent.setup();
    render(<AiApiExtractedTextLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "512");
    expect(saves()).toHaveLength(0);
    await user.tab();
    await waitFor(() => expect(saves()).toEqual([[SAVE, { kb: 512 }]]));
  });

  it.each(["1", "10240"])("GLS-FR-GLFQ: %j, an end of the range, commits", async (text) => {
    const user = userEvent.setup();
    render(<AiApiExtractedTextLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, `${text}{Enter}`);
    await waitFor(() => expect(saves()).toEqual([[SAVE, { kb: Number(text) }]]));
  });

  it.each(["0", "10241", "1.5", "-3", "abc", ""])(
    "GLS-FR-LZLG: %j shows an inline message and commits nothing",
    async (text) => {
      const user = userEvent.setup();
      render(<AiApiExtractedTextLimitRow />);
      const input = await field();
      await user.clear(input);
      if (text !== "") await user.type(input, text);
      expect(screen.getByTestId("ai-api-extracted-text-limit-invalid")).toHaveTextContent(
        "Enter a whole number from 1 to 10240.",
      );
      expect(input).toHaveAttribute("aria-invalid", "true");
      await user.type(input, "{Enter}");
      await user.tab();
      expect(saves()).toHaveLength(0);
      expect(input).toHaveValue("1024");
    },
  );

  it("GLS-FR-LZLG: a refused save restores the stored value and shows the refusal", async () => {
    refuse = "kilobytes_out_of_range";
    const user = userEvent.setup();
    render(<AiApiExtractedTextLimitRow />);
    const input = await field();
    await user.clear(input);
    await user.type(input, "500{Enter}");
    expect(await screen.findByTestId("ai-api-extracted-text-limit-error")).toHaveTextContent(
      "Enter a whole number from 1 to 10240.",
    );
    expect(input).toHaveValue("1024");
    expect(logWarn).toHaveBeenCalledWith(
      ["frontend", "ai"],
      "the extracted text limit was refused",
      expect.anything(),
    );
  });
});

describe("its place in the AI API section", () => {
  it("GLS-FR-GLFQ: stands below the History size limit, once for every provider", async () => {
    backendFor(invokeMock)();
    render(<BothLevels />);
    const level = apiLevel();
    const extracted = await level.findByTestId("ai-api-extracted-text-limit-row");
    const history = level.getByTestId("ai-api-history-limit-row");
    const tabs = level.getByRole("tablist", { name: "AI API" });
    expect(history.compareDocumentPosition(extracted) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(extracted.compareDocumentPosition(tabs) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(level.getAllByTestId("ai-api-extracted-text-limit-row")).toHaveLength(1);
    await waitFor(() => expect(commands()).toContain("load_extracted_text_limit_kb"));
  });
});
