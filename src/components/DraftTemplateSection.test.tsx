import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { DraftTemplateSection } from "./DraftTemplateSection";
import { flushDraftTemplate } from "../state/draftTemplate";
import { Settings } from "./Settings";
import { letWriteLand } from "../test/autosave";
import type { ProjectConfig } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

/** The stored project-public config a test starts from. */
function config(draftTemplate: string | null = null): ProjectConfig {
  return { lineEndings: "lf", draftTemplate, graduationConcurrencyLimit: 1 };
}

/**
 * A backend holding one project-public store, so a test can assert what was
 * persisted rather than only what was asked for.
 */
function backend(initial: ProjectConfig = config()) {
  const state = { current: initial, saves: [] as unknown[], failSave: false };
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "load_project_config") return { ...state.current };
    if (cmd === "save_project_config") {
      const patch = (args as { config: Record<string, unknown> }).config;
      state.saves.push(patch);
      if (state.failSave) throw "disk full";
      state.current = {
        ...state.current,
        lineEndings:
          (patch.lineEndings as ProjectConfig["lineEndings"]) ??
          state.current.lineEndings,
        draftTemplate:
          patch.draftTemplate === undefined
            ? state.current.draftTemplate
            : // PSS-FR-21: empty text is the unset state, never a stored value.
              (patch.draftTemplate as string) === ""
              ? null
              : (patch.draftTemplate as string),
      };
      return undefined;
    }
    return undefined;
  });
  return state;
}

const saves = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "save_project_config");
const loads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "load_project_config");

/** Put the surface on its raw-Markdown mode, which is where a test can type. */
async function source(): Promise<HTMLTextAreaElement> {
  fireEvent.click(
    await screen.findByRole("button", { name: "Edit as Markdown source" }),
  );
  return (await screen.findByLabelText(
    "Markdown source",
  )) as HTMLTextAreaElement;
}

/** Type into the raw-Markdown surface the way the author does. */
async function typeInto(area: HTMLTextAreaElement, text: string) {
  fireEvent.change(area, { target: { value: text } });
}

const status = () => screen.getByTestId("draft-template-status").textContent;

beforeEach(() => {
  invokeMock.mockReset();
});
afterEach(cleanup);

describe("Draft template section (SET-FR-16 … SET-FR-20)", () => {
  // SET-FR-03, SET-FR-16, SET-FR-17: the section reads the store, renders the Editor's own surface,
  // and offers no Save control.
  it("SET-FR-03, SET-FR-16, SET-FR-17: reads the config and renders the rich Markdown surface with no Save control", async () => {
    backend(config(null));
    render(<DraftTemplateSection contentRoot="/a#0" />);

    await waitFor(() => expect(loads()).toHaveLength(1));
    // SET-FR-16: the Editor's own surface — the WYSIWYG body and its mode
    // toggle — rather than a plain text area or a second implementation.
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    expect(document.querySelector(".ProseMirror")).not.toBeNull();
    // The editor is empty: the project configures no template.
    expect((await source()).value).toBe("");
    // SET-FR-17: nothing to save by hand.
    expect(screen.queryByRole("button", { name: /^Save/i })).toBeNull();
  });

  // SET-FR-16, SET-FR-17 tail / SET-FR-03: the section is one of the tab's sections.
  it("SET-FR-03: Draft template is a section of the Project settings tab", async () => {
    backend(config(null));
    render(
      <Settings
        contentRoot="/a#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByText("Draft template"));
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    // SET-FR-16: the section holds exactly one thing. The tab's placeholder for
    // its not-yet-built sections must not be painted under the real editor.
    expect(screen.queryByText(/stubbed in this UI kit/)).toBeNull();
  });

  // SET-FR-17, SET-FR-08, SET-FR-09, DRP-FR-06: a burst of typing is one write, reported as saving then saved.
  it("SET-FR-17, SET-FR-08, SET-FR-09, DRP-FR-06: one write follows a rest, reported saving then saved", async () => {
    const state = backend(config(null));
    render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();

    await typeInto(area, "# Context");
    // Nothing yet: the write is resting.
    expect(saves()).toHaveLength(0);

    await letWriteLand();
    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(saves()[0][1]).toEqual({
      config: { lineEndings: "lf", draftTemplate: "# Context" },
    });
    await waitFor(() => expect(status()).toContain("Saved"));
    expect(state.current.draftTemplate).toBe("# Context");
    // Still no Save control, and no dirty marker for the tab to act on.
    expect(screen.queryByRole("button", { name: /^Save/i })).toBeNull();
  });

  /**
   * SET-FR-17, DRP-FR-06 tail / SET-FR-08 / SET-FR-09: the section takes no part in the
   * tab's per-section dirty state and raises no discard-unsaved-changes
   * confirmation — there is nothing to discard, because what the author typed
   * is already on its way to disk. And SET-FR-17's other immediate write:
   * navigating away from the section WITHIN the tab.
   */
  it("SET-FR-17, SET-FR-08, SET-FR-09, DRP-FR-06: navigating away writes at once, with no dirty state and no discard prompt", async () => {
    const state = backend(config(null));
    render(
      <Settings
        contentRoot="/a#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByText("Draft template"));
    const area = await source();
    await typeInto(area, "# Context");

    // Still resting — and the section offers nothing to save or discard by hand.
    expect(saves()).toHaveLength(0);
    expect(screen.queryByRole("button", { name: /^Save/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /discard/i })).toBeNull();

    // Leaving the section inside the tab writes at once.
    await userEvent.click(screen.getByText("Plugins"));
    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(state.current.draftTemplate).toBe("# Context");
    // No confirmation was raised on the way out.
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText(/unsaved/i)).toBeNull();

    // And coming back shows what was persisted, read afresh.
    await userEvent.click(screen.getByText("Draft template"));
    expect((await source()).value).toBe("# Context");
  });

  // PSS-FR-21 / SET-FR-19: deleting every character persists the UNSET state.
  it("SET-FR-19, PSS-FR-21: an emptied editor persists the unset state, not an empty template", async () => {
    const state = backend(config("# Context"));
    render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();
    expect(area.value).toBe("# Context");

    await typeInto(area, "");
    await letWriteLand();

    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(
      (saves()[0][1] as { config: { draftTemplate: string } }).config
        .draftTemplate,
    ).toBe("");
    // The store treats that as unset, which is what a draft created afterwards
    // opens empty on (DRS-FR-06).
    expect(state.current.draftTemplate).toBeNull();
  });

  // SET-FR-18: a failed write keeps the author's content and offers a retry.
  it("SET-FR-18: a failed write keeps the text, reports the failure, and retries", async () => {
    const state = backend(config(null));
    state.failSave = true;
    render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();

    await typeInto(area, "# Context");
    await letWriteLand();

    await waitFor(() => expect(status()).toContain("could not be saved"));
    // Nothing was reverted, reloaded, or replaced by what is on disk: the
    // buffer still holds exactly what was typed. (The store *was* read again —
    // that is the write's own read-modify-write, which carries every other
    // project-public section through unchanged — but nothing it read reached
    // the editor.)
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("# Context");

    state.failSave = false;
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(status()).toContain("Saved"));
    expect(state.current.draftTemplate).toBe("# Context");
  });

  // SET-FR-18, PSS-FR-10: a malformed store is stated rather than presented as an empty
  // editor, and nothing is written from the section.
  it("SET-FR-18, PSS-FR-10: a failed load states the problem and mounts no editor", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_config") throw "malformed project config";
      return undefined;
    });
    const { unmount } = render(<DraftTemplateSection contentRoot="/a#0" />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("could not be read");
    // No editing surface at all, so nothing here can overwrite the store.
    expect(document.querySelector(".ProseMirror")).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Edit as Markdown source" }),
    ).toBeNull();
    expect(saves()).toHaveLength(0);
    // Not even on the way out (SET-FR-17's leave-the-section write).
    unmount();
    await letWriteLand();
    expect(saves()).toHaveLength(0);

    // And a repaired store shows the configured template on the next open.
    cleanup();
    backend(config("# Context"));
    render(<DraftTemplateSection contentRoot="/a#0" />);
    expect((await source()).value).toBe("# Context");
  });

  // SET-FR-20, SWN-FR-02: the section never shows a template that is not the active
  // worktree's own.
  it("SET-FR-20, SWN-FR-02: a project change re-reads, showing a loading state and never the outgoing template", async () => {
    let held: ((c: ProjectConfig) => void) | null = null;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "load_project_config") return undefined;
      if (held === null) {
        // Project A answers at once.
        held = () => {};
        return config("# A's template");
      }
      // Project B's read is held, so the loading state is observable.
      return new Promise<ProjectConfig>((resolve) => {
        held = resolve;
      });
    });

    const view = render(<DraftTemplateSection contentRoot="/a#0" />);
    expect((await source()).value).toBe("# A's template");

    view.rerender(<DraftTemplateSection contentRoot="/b#0" />);

    // While the read is outstanding: a loading state, no editing surface, and
    // at no moment A's template.
    expect(screen.getByRole("status")).toHaveTextContent(/Loading/);
    expect(screen.queryByLabelText("Markdown source")).toBeNull();
    expect(document.querySelector(".ProseMirror")).toBeNull();
    expect(document.body.textContent).not.toContain("A's template");

    await waitFor(() => expect(held).not.toBeNull());
    held!(config(null));

    // B configures none, so the editor lands empty.
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Edit as Markdown source" }),
      ).not.toBeNull(),
    );
    expect((await source()).value).toBe("");
  });

  /**
   * SET-FR-17 / SET-FR-20: a project or worktree change must not cost the
   * author what they had just typed. The section drops the store it holds when
   * the content root changes, so the pending write has to be brought forward
   * first rather than discarded with it.
   */
  it("SET-FR-17: a content-root change writes the pending edit before dropping the store", async () => {
    const state = backend(config(null));
    const view = render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();

    await typeInto(area, "# Context");
    expect(saves()).toHaveLength(0);

    // The switch arrives inside the rest, which is the whole of the risk.
    view.rerender(<DraftTemplateSection contentRoot="/b#0" />);

    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(state.current.draftTemplate).toBe("# Context");
  });

  /**
   * SET-FR-17: the shell's teardown path can bring the write forward and WAIT
   * for it, so a worktree switch never races it to the wrong content root.
   */
  it("SET-FR-17: the teardown flush writes the pending edit and awaits it", async () => {
    const state = backend(config(null));
    render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();

    await typeInto(area, "# Context");
    expect(saves()).toHaveLength(0);

    await flushDraftTemplate();
    expect(saves()).toHaveLength(1);
    expect(state.current.draftTemplate).toBe("# Context");

    // And a second flush with nothing outstanding writes nothing.
    await flushDraftTemplate();
    expect(saves()).toHaveLength(1);
  });

  // With no section mounted there is nothing to flush, and a teardown must not
  // trip over that.
  it("SET-FR-17: the teardown flush is inert when no section is mounted", async () => {
    backend(config(null));
    const { unmount } = render(<DraftTemplateSection contentRoot="/a#0" />);
    await source();
    unmount();
    // SWN-FR-09: the flush reports whether the write landed, so a settings
    // window's save-before-close sweep can cancel on a failure. With nothing
    // mounted there is nothing to write, which is a success and not a refusal.
    await expect(flushDraftTemplate()).resolves.toBe(true);
    expect(saves()).toHaveLength(0);
  });

  // SET-FR-17: the write also happens immediately when the section is left or
  // the tab closes — both of which arrive here as an unmount.
  it("SET-FR-17: leaving the section writes the pending edit at once", async () => {
    const state = backend(config(null));
    const { unmount } = render(<DraftTemplateSection contentRoot="/a#0" />);
    const area = await source();

    await typeInto(area, "# Context");
    expect(saves()).toHaveLength(0);

    unmount();
    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(state.current.draftTemplate).toBe("# Context");
  });
});
