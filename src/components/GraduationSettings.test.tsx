import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "./Settings";
import {
  resetSettingsSections,
  runSettingsSaveSweep,
  settingsSectionsPending,
} from "../state/settingsSweep";

// SET-FR-QKKQ through SET-FR-IHQE: the Graduation section of Project settings.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Limit = number | "unlimited";

interface Backend {
  /** What `load_project_config` reports. `undefined` leaves the field out. */
  stored?: Limit;
  saveFails?: boolean;
  loadFails?: boolean;
  /** Hold the read until the test releases it. */
  hold?: Promise<void>;
}

function backend(options: Backend = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_project_config") {
      if (options.hold) await options.hold;
      if (options.loadFails) throw new Error("malformed project config");
      return {
        lineEndings: "lf",
        draftTemplate: null,
        ...(options.stored === undefined
          ? {}
          : { graduationConcurrencyLimit: options.stored }),
      };
    }
    if (cmd === "save_project_config") {
      if (options.saveFails) throw new Error("disk is full");
      return undefined;
    }
    return undefined;
  });
}

function saves(): unknown[] {
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "save_project_config")
    .map(([, args]) => (args as { config: unknown }).config);
}

beforeEach(() => {
  invokeMock.mockReset();
  resetSettingsSections();
});

afterEach(cleanup);

async function openGraduation() {
  render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
  await userEvent.click(screen.getByRole("tab", { name: "Graduation" }));
}

const select = () => screen.findByRole("combobox", { name: "Concurrent graduation runs" });

function optionLabels(box: HTMLElement): string[] {
  return within(box)
    .getAllByRole("option")
    .map((option) => option.textContent ?? "");
}

describe("Project settings — Graduation section", () => {
  it("SET-FR-03: the window lists a Graduation section", async () => {
    backend();
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    expect(screen.getByRole("tab", { name: "Graduation" })).toBeInTheDocument();
  });

  it("SET-FR-QKKQ, SET-FR-ZNXA: the drop-down offers 1, 2, 4, 8 and Unlimited, and rests on 1", async () => {
    backend();
    await openGraduation();
    const box = await select();
    expect(optionLabels(box)).toEqual(["1", "2", "4", "8", "Unlimited"]);
    expect(box).toHaveValue("1");
  });

  it("SET-FR-ZNXA: a stored limit the list does not hold is offered, in numeric order, and stays selected", async () => {
    backend({ stored: 3 });
    await openGraduation();
    const box = await select();
    expect(optionLabels(box)).toEqual(["1", "2", "3", "4", "8", "Unlimited"]);
    expect(box).toHaveValue("3");
    expect(saves(), "reading writes nothing").toEqual([]);
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("SET-FR-ZNXA: a stored Unlimited is selected as the named value", async () => {
    backend({ stored: "unlimited" });
    await openGraduation();
    expect(await select()).toHaveValue("unlimited");
  });

  it("SET-FR-HQKK: a change marks the section pending and writes nothing at once", async () => {
    backend();
    await openGraduation();
    await userEvent.selectOptions(await select(), "4");

    expect(settingsSectionsPending()).toEqual(["graduation"]);
    expect(saves()).toEqual([]);
    expect(screen.getByTestId("graduation-pending")).toHaveTextContent("not saved yet");
  });

  it("SET-FR-HQKK: choosing the stored value again leaves nothing pending", async () => {
    backend({ stored: 2 });
    await openGraduation();
    const box = await select();
    await userEvent.selectOptions(box, "8");
    await userEvent.selectOptions(box, "2");
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("SET-FR-HQKK, SET-FR-IHQE: the window's save writes the limit alone, from another section too", async () => {
    backend();
    await openGraduation();
    await userEvent.selectOptions(await select(), "8");

    await userEvent.click(screen.getByRole("tab", { name: "Plugins" }));
    expect(settingsSectionsPending(), "leaving the section keeps the change").toEqual([
      "graduation",
    ]);

    await expect(runSettingsSaveSweep()).resolves.toEqual({ ok: true });
    expect(saves()).toEqual([{ graduationConcurrencyLimit: 8 }]);
    expect(settingsSectionsPending()).toEqual([]);

    await userEvent.click(screen.getByRole("tab", { name: "Graduation" }));
    expect(await select()).toHaveValue("8");
  });

  it("SET-FR-IHQE: the choice survives a visit to another section", async () => {
    backend();
    await openGraduation();
    await userEvent.selectOptions(await select(), "2");
    await userEvent.click(screen.getByRole("tab", { name: "MCP servers" }));
    await userEvent.click(screen.getByRole("tab", { name: "Graduation" }));
    expect(await select()).toHaveValue("2");
    expect(screen.getByTestId("graduation-pending")).toBeInTheDocument();
  });

  it("SET-FR-HQKK: Unlimited is written as the named value, not as a number", async () => {
    backend();
    await openGraduation();
    await userEvent.selectOptions(await select(), "unlimited");
    await runSettingsSaveSweep();
    expect(saves()).toEqual([{ graduationConcurrencyLimit: "unlimited" }]);
  });

  it("SET-FR-TFNG: the control shows a loading state and takes no choice until the read lands", async () => {
    let release = () => {};
    const hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    backend({ hold, stored: 4 });
    await openGraduation();

    expect(screen.getByRole("status")).toHaveTextContent("Loading");
    expect(screen.queryByRole("combobox")).toBeNull();

    release();
    expect(await select()).toHaveValue("4");
  });

  it("SET-FR-TFNG: a failed read says so, offers no choice, and shows no limit of one", async () => {
    backend({ loadFails: true });
    await openGraduation();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("could not be read");
    expect(alert).toHaveTextContent("malformed project config");
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("SET-FR-TFNG: a failed save keeps the selection, shows the error, and stays pending", async () => {
    backend({ saveFails: true });
    await openGraduation();
    await userEvent.selectOptions(await select(), "4");

    await expect(runSettingsSaveSweep()).resolves.toEqual({
      ok: false,
      section: "graduation",
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("disk is full");
    expect(await select()).toHaveValue("4");
    expect(settingsSectionsPending()).toEqual(["graduation"]);
  });

  it("SET-FR-TFNG: Retry writes the kept selection and clears the pending state", async () => {
    let fail = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_config") return { lineEndings: "lf", draftTemplate: null };
      if (cmd === "save_project_config") {
        if (fail) throw new Error("disk is full");
        return undefined;
      }
      return undefined;
    });
    await openGraduation();
    await userEvent.selectOptions(await select(), "2");
    await runSettingsSaveSweep();
    await screen.findByRole("alert");

    fail = false;
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(settingsSectionsPending()).toEqual([]));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(saves()).toHaveLength(2);
    expect(await select()).toHaveValue("2");
  });

  it("SET-FR-TFNG: a section that is saving states it and takes no further choice", async () => {
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_config") return { lineEndings: "lf", draftTemplate: null };
      if (cmd === "save_project_config") await gate;
      return undefined;
    });
    await openGraduation();
    await userEvent.selectOptions(await select(), "2");
    const sweep = runSettingsSaveSweep();

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Saving"));
    expect(await select()).toBeDisabled();
    expect(settingsSectionsPending(), "a write in flight is pending").toEqual(["graduation"]);

    release();
    await sweep;
    await waitFor(() => expect(settingsSectionsPending()).toEqual([]));
  });
});
