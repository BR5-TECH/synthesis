import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { aiErrorMessage } from "./AiIntegrations";
import { turnTimeoutToCommit } from "./AiApiTurnTimeoutRow";
import { AI_ERRORS, type AiApiIntegration, type AiApiProviderId } from "../types";
import * as api from "../api";
import { logWarn } from "../logging";
import {
  API_ALL,
  BothLevels,
  apiIntegration,
  apiLevel,
  argsOfFor,
  backendFor,
  commandsFor,
  openApiTab,
} from "../test/aiIntegrationsFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("../logging", async (original) => ({
  ...(await original<typeof import("../logging")>()),
  logWarn: vi.fn(),
}));

const backend = backendFor(invokeMock);
const commands = commandsFor(invokeMock);
const argsOf = argsOfFor(invokeMock);
const SET = "set_ai_api_turn_timeout";

/** The backend's answer to a committed value: the record carrying it. */
const stores = (args?: Record<string, unknown>) =>
  apiIntegration(args!.provider as AiApiProviderId, {
    turnTimeoutMs: args!.timeoutMs as number | null,
  });

/** Every provider, OpenRouter holding `stored`. */
function withOpenRouter(stored: number | null): AiApiIntegration[] {
  return API_ALL.map((p) =>
    apiIntegration(p, { turnTimeoutMs: p === "openrouter" ? stored : null }),
  );
}

const setCalls = () => commands().filter((c) => c === SET).length;

async function field() {
  return (await apiLevel().findByLabelText("Turn timeout")) as HTMLInputElement;
}

beforeEach(() => {
  invokeMock.mockReset();
  vi.mocked(logWarn).mockClear();
});

afterEach(cleanup);

describe("turnTimeoutToCommit", () => {
  it.each([
    ["", null],
    ["   ", null],
    ["29", undefined],
    ["30", 30_000],
    ["  120  ", 120_000],
    ["030", 30_000],
    ["3600", 3_600_000],
    ["3601", undefined],
    ["0", undefined],
    ["30.5", undefined],
    ["300.0", undefined],
    ["-30", undefined],
    ["+300", undefined],
    ["300s", undefined],
    ["3e2", undefined],
    ["abc", undefined],
    ["٣٠٠", undefined],
    ["99999999999999999999", undefined],
  ])("AII-FR-QSOR: %j commits %j", (text, expected) => {
    expect(turnTimeoutToCommit(text)).toBe(expected);
  });
});

describe("the turn timeout row", () => {
  it("AII-FR-QSOR, AII-FR-28, AAP-FR-FGNK: a whole number committed on blur is stored at once in milliseconds", async () => {
    backend({ api: withOpenRouter(null) }, { [SET]: stores });
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    expect(input.value).toBe("");
    expect(input.placeholder).toBe("300");
    expect(input.parentElement).toHaveTextContent(/^s$/);
    await user.type(input, "120");
    expect(setCalls()).toBe(0);
    await user.tab();

    await waitFor(() => expect(setCalls()).toBe(1));
    expect(argsOf(SET)).toEqual({ provider: "openrouter", timeoutMs: 120_000 });
    expect((await field()).value).toBe("120");
  });

  it("AII-FR-QSOR: Enter commits, and a blur after it sends no second call", async () => {
    let resolve: (value: AiApiIntegration) => void = () => {};
    backend(
      { api: withOpenRouter(null) },
      {
        [SET]: () => new Promise<AiApiIntegration>((r) => (resolve = r)),
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    await user.type(input, "120{Enter}");
    await user.tab();
    resolve(apiIntegration("openrouter", { turnTimeoutMs: 120_000 }));

    await waitFor(() => expect(input.value).toBe("120"));
    expect(setCalls()).toBe(1);
  });

  it.each([
    ["30", 30_000],
    ["3600", 3_600_000],
  ])("AII-FR-QSOR: the bound %s is committed", async (typed, ms) => {
    backend({ api: withOpenRouter(null) }, { [SET]: stores });
    render(<BothLevels />);
    const user = userEvent.setup();

    await user.type(await field(), typed);
    await user.tab();

    await waitFor(() => expect(argsOf(SET)).toEqual({ provider: "openrouter", timeoutMs: ms }));
  });

  it.each(["29", "3601", "30.5", "-30", "0", "300s", "abc"])(
    "AII-FR-QSOR, AII-FR-QTZF: %j is not committed and leaving restores the stored value",
    async (typed) => {
      backend({ api: withOpenRouter(600_000) }, { [SET]: stores });
      render(<BothLevels />);
      const user = userEvent.setup();

      const input = await field();
      await user.clear(input);
      await user.type(input, `${typed}{Enter}`);
      expect(input).toHaveAttribute("aria-invalid", "true");
      await user.tab();

      await waitFor(() => expect(input.value).toBe("600"));
      expect(input).not.toHaveAttribute("aria-invalid");
      expect(setCalls()).toBe(0);
    },
  );

  it("AII-FR-QSOR: an empty field clears the stored value", async () => {
    backend({ api: withOpenRouter(600_000) }, { [SET]: stores });
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    await user.clear(input);
    await user.type(input, "   ");
    await user.tab();

    await waitFor(() => expect(argsOf(SET)).toEqual({ provider: "openrouter", timeoutMs: null }));
    await waitFor(() => expect(input.value).toBe(""));
  });

  it("AII-FR-QSOR: the stored value, however it is spelled, sends nothing", async () => {
    backend({ api: withOpenRouter(300_000) }, { [SET]: stores });
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    await user.clear(input);
    await user.type(input, "0300");
    await user.tab();
    await waitFor(() => expect(input.value).toBe("300"));

    await user.click(input);
    await user.tab();
    expect(setCalls()).toBe(0);
  });

  it("AII-FR-QSOR: a stored value that is not whole seconds is shown and is not an error", async () => {
    backend({ api: withOpenRouter(45_500) }, { [SET]: stores });
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    expect(input.value).toBe("45.5");
    expect(input).not.toHaveAttribute("aria-invalid");
    await user.click(input);
    await user.tab();
    expect(setCalls()).toBe(0);
  });

  it("AII-FR-28: a refusal keeps the stored value, shows the error beside the field, and logs it", async () => {
    let refuse = true;
    backend(
      { api: withOpenRouter(600_000) },
      {
        [SET]: (args) => {
          if (refuse) throw AI_ERRORS.turnTimeoutOutOfRange;
          return stores(args);
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    const input = await field();
    await user.clear(input);
    await user.type(input, "120");
    await user.tab();

    const error = await apiLevel().findByTestId("ai-api-turn-timeout-error");
    expect(error).toHaveTextContent(aiErrorMessage(AI_ERRORS.turnTimeoutOutOfRange));
    expect(aiErrorMessage(AI_ERRORS.turnTimeoutOutOfRange)).toBe(
      "Enter a whole number of seconds from 30 to 3600.",
    );
    expect(input.value).toBe("600");
    expect(logWarn).toHaveBeenCalledWith(["frontend", "ai"], "ai api turn timeout refused", {
      provider: "openrouter",
      error: "turn_timeout_out_of_range",
    });

    // The next accepted commit clears the error.
    refuse = false;
    await user.clear(input);
    await user.type(input, "150");
    await user.tab();
    await waitFor(() =>
      expect(apiLevel().queryByTestId("ai-api-turn-timeout-error")).toBeNull(),
    );
    expect(argsOf(SET)).toEqual({ provider: "openrouter", timeoutMs: 150_000 });
  });

  it("AII-FR-05, AII-FR-QTZF: every tab shows its own value and the explanation line", async () => {
    backend({
      api: API_ALL.map((p) =>
        apiIntegration(p, {
          turnTimeoutMs: { openai: 90_000, custom: 3_600_000 }[p as "openai"] ?? null,
        }),
      ),
    });
    render(<BothLevels />);
    const user = userEvent.setup();

    for (const [name, shown] of [
      [/OpenAI/, "90"],
      [/Anthropic/, ""],
      [/Custom/, "3600"],
    ] as const) {
      await openApiTab(user, apiLevel(), name);
      expect((await field()).value).toBe(shown);
      expect(apiLevel().getByTestId("ai-api-turn-timeout-note")).toHaveTextContent(
        "Bounds each agent conversation turn on this provider. Empty uses the " +
          "project's execution timeout, or 5 minutes.",
      );
    }
  });

  it("AII-FR-28: an entry commits to the tab it was typed in, and an error stays on that tab", async () => {
    backend(
      { api: withOpenRouter(null) },
      {
        [SET]: (args) => {
          if (args!.provider === "openai") throw AI_ERRORS.turnTimeoutOutOfRange;
          return stores(args);
        },
      },
    );
    render(<BothLevels />);
    const user = userEvent.setup();

    await openApiTab(user, apiLevel(), /OpenAI/);
    await user.type(await field(), "120");
    await openApiTab(user, apiLevel(), /Anthropic/);

    await waitFor(() => expect(argsOf(SET)).toEqual({ provider: "openai", timeoutMs: 120_000 }));
    expect((await field()).value).toBe("");
    expect(apiLevel().queryByTestId("ai-api-turn-timeout-error")).toBeNull();

    // The refusal arrived after the OpenAI field was gone, and it waits there.
    await openApiTab(user, apiLevel(), /OpenAI/);
    expect(await apiLevel().findByTestId("ai-api-turn-timeout-error")).toHaveTextContent(
      aiErrorMessage(AI_ERRORS.turnTimeoutOutOfRange),
    );
    expect((await field()).value).toBe("");
  });

  it("AII-FR-QSOR: a record from an older backend, with no field, renders an empty field", async () => {
    const older = API_ALL.map((p) => {
      const { turnTimeoutMs: _omitted, ...rest } = apiIntegration(p);
      return rest as unknown as AiApiIntegration;
    });
    backend({ api: older });
    render(<BothLevels />);

    expect((await field()).value).toBe("");
  });

  it("AAP-FR-FGNK: the wrapper names the command and its arguments", async () => {
    invokeMock.mockResolvedValue(apiIntegration("custom"));
    await api.setAiApiTurnTimeout("custom", null);
    expect(invokeMock).toHaveBeenCalledWith(SET, { provider: "custom", timeoutMs: null });
  });

  it("a level with no records renders no turn timeout field", async () => {
    backend({ api: [] });
    render(<BothLevels />);
    await screen.findByTestId("ai-api-level");
    expect(apiLevel().queryByLabelText("Turn timeout")).toBeNull();
  });
});
