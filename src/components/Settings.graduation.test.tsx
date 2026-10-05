import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "./Settings";
import { timeLimitToCommit } from "./GraduationSettingsSection";

// SET-FR-DHFS / SET-FR-YMRV: the Project settings Graduation section — the one
// place the execution time limit is set, and where a run stopped at that limit
// routes the author (GRU-FR-QLRQ).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

interface Saved {
  config: Record<string, unknown>;
}

function backend(stored: number | null, refuse?: string) {
  const saves: Saved[] = [];
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "load_project_config":
        return {
          lineEndings: "crlf",
          draftTemplate: "# Template",
          graduationConcurrencyLimit: 3,
          executionTimeoutMs: stored,
        };
      case "save_project_config":
        if (refuse) throw refuse;
        saves.push(args as unknown as Saved);
        stored = (args as unknown as Saved).config.executionTimeoutMs as number | null;
        return undefined;
      default:
        return undefined;
    }
  });
  return saves;
}

beforeEach(() => invokeMock.mockReset());
afterEach(cleanup);

async function openGraduation(requested: string | null = null) {
  render(
    <Settings
      contentRoot="/p#0"
      lineEndings="crlf"
      onSelectLineEndings={vi.fn()}
      sectionRequest={requested ? { section: requested, nonce: 1 } : null}
    />,
  );
  if (!requested) await userEvent.click(screen.getByText("Graduation"));
  const field = await screen.findByTestId("graduation-time-limit");
  await waitFor(() => expect(field).not.toBeDisabled());
  return field as HTMLInputElement;
}

describe("Project settings — Graduation", () => {
  it("SET-FR-03, SET-FR-DHFS: the section holds the time limit in minutes, with what it bounds", async () => {
    backend(90 * 60_000);
    const field = await openGraduation();
    expect(field.value).toBe("90");
    expect(screen.getByLabelText("Execution time limit")).toBe(field);
    expect(screen.getByTestId("graduation-time-limit-note")).toHaveTextContent(
      "Bounds each graduation turn, and each conversation turn whose provider sets no turn timeout.",
    );
  });

  it("SET-FR-DHFS: an unset limit shows the two-hour default as a placeholder", async () => {
    backend(null);
    const field = await openGraduation();
    expect(field.value).toBe("");
    expect(field.placeholder).toBe("120");
  });

  it("SET-FR-DHFS, GRU-FR-QLRQ, SWN-FR-13: a request naming the section opens on it", async () => {
    backend(null);
    const field = await openGraduation("graduation");
    expect(field).toBeInTheDocument();
  });

  it("SET-FR-YMRV: Enter persists at once and carries the other stored values through", async () => {
    const saves = backend(null);
    const field = await openGraduation();
    await userEvent.type(field, "180{Enter}");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].config).toEqual({
      lineEndings: "crlf",
      executionTimeoutMs: 180 * 60_000,
    });
    // SET-FR-YMRV: outside the section dirty state — nothing to save on close.
    expect(screen.queryByRole("button", { name: /save/i })).toBeNull();
  });

  it("SET-FR-YMRV: blur commits, and an empty entry stores the unset value", async () => {
    const saves = backend(45 * 60_000);
    const field = await openGraduation();
    await userEvent.clear(field);
    field.blur();
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].config.executionTimeoutMs).toBeNull();
  });

  it.each(["0", "361", "1.5", "two"])(
    "SET-FR-YMRV: %s is refused inline and the stored value stays",
    async (entry) => {
      const saves = backend(60 * 60_000);
      const field = await openGraduation();
      await userEvent.clear(field);
      await userEvent.type(field, `${entry}{Enter}`);
      expect(await screen.findByTestId("graduation-time-limit-error")).toHaveTextContent(
        "Enter a whole number of minutes from 1 to 360",
      );
      expect(saves).toHaveLength(0);
    },
  );

  it("SET-FR-YMRV: a refused write keeps the stored value and says why", async () => {
    backend(60 * 60_000, "project.toml could not be written");
    const field = await openGraduation();
    await userEvent.clear(field);
    await userEvent.type(field, "30{Enter}");
    expect(await screen.findByTestId("graduation-time-limit-error")).toHaveTextContent(
      "project.toml could not be written",
    );
    expect(field.value).toBe("60");
  });

  it("SET-FR-YMRV: Enter and the blur after it write once", async () => {
    const saves = backend(null);
    const field = await openGraduation();
    await userEvent.type(field, "180{Enter}");
    await userEvent.tab();
    await waitFor(() => expect(saves).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(saves).toHaveLength(1);
  });

  it("SET-FR-YMRV: the stored value typed again writes nothing", async () => {
    const saves = backend(60 * 60_000);
    const field = await openGraduation();
    await userEvent.clear(field);
    await userEvent.type(field, "60{Enter}");
    field.blur();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(saves).toHaveLength(0);
    expect(screen.queryByTestId("graduation-time-limit-error")).toBeNull();
  });

  it("SET-FR-YMRV: a stored value that is not whole minutes is shown and never refused untouched", async () => {
    const saves = backend(90_500);
    const field = await openGraduation();
    expect(field.value).toBe(String(90_500 / 60_000));
    field.focus();
    field.blur();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(saves).toHaveLength(0);
    expect(screen.queryByTestId("graduation-time-limit-error")).toBeNull();
  });

  it("SET-FR-YMRV: an entry with spaces around it commits its minutes", async () => {
    const saves = backend(null);
    const field = await openGraduation();
    await userEvent.type(field, " 30 {Enter}");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0].config.executionTimeoutMs).toBe(30 * 60_000);
  });

  it("SET-FR-DHFS: a limit that cannot be read says so instead of a field", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_config") throw "project.toml is not valid TOML";
      return undefined;
    });
    render(<Settings contentRoot="/p#0" lineEndings="crlf" onSelectLineEndings={vi.fn()} />);
    await userEvent.click(screen.getByText("Graduation"));
    expect(await screen.findByTestId("graduation-time-limit-load-error")).toHaveTextContent(
      "project.toml is not valid TOML",
    );
  });

  it("SET-FR-03: Graduation is listed in the section nav after Docker", async () => {
    backend(null);
    render(<Settings contentRoot="/p#0" lineEndings="crlf" onSelectLineEndings={vi.fn()} />);
    const nav = screen.getByText("Graduation").closest("nav") ?? document.body;
    const text = nav.textContent ?? "";
    expect(text.indexOf("Docker")).toBeGreaterThanOrEqual(0);
    expect(text.indexOf("Graduation")).toBeGreaterThan(text.indexOf("Docker"));
  });

  it("SET-FR-YMRV: the bounds of a committed entry", () => {
    expect(timeLimitToCommit("")).toBeNull();
    expect(timeLimitToCommit("  ")).toBeNull();
    expect(timeLimitToCommit("1")).toBe(60_000);
    expect(timeLimitToCommit("360")).toBe(21_600_000);
    expect(timeLimitToCommit("0")).toBeUndefined();
    expect(timeLimitToCommit("361")).toBeUndefined();
    expect(timeLimitToCommit("-5")).toBeUndefined();
    expect(timeLimitToCommit("1e2")).toBeUndefined();
    expect(timeLimitToCommit(" 30 ")).toBe(30 * 60_000);
  });
});
