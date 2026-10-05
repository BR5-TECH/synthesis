import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  NewTypedArtifactModal,
  type NewTypedArtifactPayload,
  type SubmitResult,
} from "./NewTypedArtifactModal";
import { ARTIFACT_TYPES } from "../artifactTypes";
import type { FolderOption } from "../hooks/useProjectFolders";
import type { NewTypedArtifactSeed } from "../types";

// NTA-FR-04: every folder in the project is a candidate location, including
// ones the Project panel's active lens hides — `scenarios` is empty and
// `tooling` unclassified, so **All artifacts** would show neither, yet both
// must be selectable here.
const FOLDERS: FolderOption[] = [
  { path: "scenarios", label: "scenarios" },
  { path: "specifications", label: "specifications" },
  { path: "specifications/ui", label: "specifications/ui" },
  { path: "tooling", label: "tooling" },
];

function renderModal(seed: NewTypedArtifactSeed = {}, folders = FOLDERS) {
  const onClose = vi.fn();
  const onSubmit = vi.fn<
    (p: NewTypedArtifactPayload) => Promise<SubmitResult>
  >(async () => ({ ok: true }));
  render(
    <NewTypedArtifactModal
      seed={seed}
      folders={folders}
      onClose={onClose}
      onSubmit={onSubmit}
    />,
  );
  return { onClose, onSubmit };
}

const location = () => screen.getByLabelText("Location (required)");
const name = () => screen.getByLabelText("Name (required)");
const type = () => screen.getByLabelText("Artifact Type (required)");
const createButton = () => screen.getByRole("button", { name: /Create/ });

afterEach(() => cleanup());

describe("New Artifact modal (NTA-new-typed-artifact.md)", () => {
  // NTA-FR-01, NTA-FR-02, NTA-FR-03: exactly the three creation inputs, in the shared centered-overlay
  // chrome — and none of the things this window deliberately does not carry.
  it("NTA-FR-01, NTA-FR-02, NTA-FR-03: presents Location, Name and Artifact Type in a modal and nothing else", () => {
    renderModal();

    const dialog = screen.getByRole("dialog", { name: "New Artifact" });
    // NTA-FR-01 / NTA-FR-02: a centered modal overlay, not a tab and not a
    // panel. The frame is re-mounted per open, so it re-centers each time.
    expect(dialog.closest(".scrim")).not.toBeNull();

    expect(location()).toBeInTheDocument();
    expect(name()).toBeInTheDocument();
    expect(type()).toBeInTheDocument();

    // NTA-FR-03: no Markdown editor, no initial-content editor, no AI-prompt
    // block, no draft control and no draft-template control.
    expect(screen.queryByLabelText("Content")).not.toBeInTheDocument();
    expect(screen.queryByText("AI prompt")).not.toBeInTheDocument();
    expect(screen.queryByText(/template/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/draft/i)).not.toBeInTheDocument();
    expect(document.querySelector(".ProseMirror")).toBeNull();
    expect(screen.queryByRole("textbox", { name: /content/i })).toBeNull();
    // Stated positively as well, because a negative over a component that has
    // no such strings by construction would not catch a fourth input labelled
    // something else: exactly three fields, two of them selects and one a text
    // box (NTA-FR-03).
    expect(dialog.querySelectorAll(".picker-field")).toHaveLength(3);
    expect(screen.getAllByRole("combobox")).toHaveLength(2);
    expect(screen.getAllByRole("textbox")).toHaveLength(1);
  });

  // NTA-FR-04, NTA-FR-09 / NTA-FR-07: the File-menu flow's starting state.
  it("NTA-FR-04, NTA-FR-07, NTA-FR-09: opens on the project root with an empty name, no type, and Create disabled", async () => {
    renderModal();

    expect(location()).toHaveValue("");
    expect(name()).toHaveValue("");
    expect(type()).toHaveValue("");
    expect(createButton()).toBeDisabled();

    // The location is the user's to change in every flow.
    await userEvent.selectOptions(location(), "specifications");
    expect(location()).toHaveValue("specifications");
  });

  // NTA-FR-06 head / NTA-FR-08: the Project context-menu flow starts at the
  // right-clicked folder, which stays editable, and seeds no type from it.
  it("NTA-FR-06, NTA-FR-08: seeds the location from the invoking folder and never the type", async () => {
    renderModal({ initialLocation: "specifications" });

    expect(location()).toHaveValue("specifications");
    expect(type()).toHaveValue("");

    await userEvent.selectOptions(location(), "tooling");
    expect(location()).toHaveValue("tooling");
  });

  // NTA-FR-04: a seeded folder the published list has not caught up with is
  // still what the window opens on, rather than silently falling back to root.
  it("offers a seeded location the folder list does not carry yet", () => {
    renderModal({ initialLocation: "brand/new" }, FOLDERS);
    expect(location()).toHaveValue("brand/new");
  });

  // NTA-FR-04: the lens the Project panel is on hides nothing here.
  it("NTA-FR-04: lists the project root and every folder, hidden ones included", () => {
    renderModal();

    const values = within(location() as HTMLSelectElement)
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(values).toEqual([
      "",
      "scenarios",
      "specifications",
      "specifications/ui",
      "tooling",
    ]);
  });

  // NTA-FR-06, NTA-FR-09: exactly the eight built-in types, with no unset entry among
  // them, and Create gated on one being chosen.
  it("NTA-FR-06, NTA-FR-09: offers exactly the eight built-in types with no unset entry", async () => {
    renderModal();

    const options = within(type() as HTMLSelectElement).getAllByRole(
      "option",
    ) as HTMLOptionElement[];
    // The placeholder is a prompt to choose rather than a selectable value.
    expect(options[0].value).toBe("");
    expect(options[0].disabled).toBe(true);
    expect(options.slice(1).map((o) => o.value)).toEqual(
      ARTIFACT_TYPES.map((t) => t.value),
    );
    expect(options.slice(1).map((o) => o.textContent)).toEqual([
      "Skill",
      "Agent",
      "Prompt",
      "Spec",
      "Flow",
      "Instructions",
      "Scenario",
      "Scratchpad",
    ]);
    expect(options).toHaveLength(9);

    // A valid name with no type chosen is still not creatable.
    await userEvent.type(name(), "login.scenario.md");
    expect(createButton()).toBeDisabled();
    await userEvent.selectOptions(type(), "scenario");
    expect(createButton()).toBeEnabled();
  });

  // NTA-FR-05, NTA-FR-09: the name is the complete basename, and a malformed one makes no
  // call at all.
  it("NTA-FR-05, NTA-FR-09: takes the name verbatim and refuses an empty one or a path", async () => {
    const { onSubmit } = renderModal();

    await userEvent.selectOptions(type(), "scenario");
    await userEvent.type(name(), "login.scenario.md");
    expect(createButton()).toBeEnabled();
    await userEvent.click(createButton());
    expect(onSubmit).toHaveBeenCalledWith({
      location: null,
      // Nothing appended, and no extension derived from the type.
      name: "login.scenario.md",
      artifactType: "scenario",
    });

    onSubmit.mockClear();
    await userEvent.clear(name());
    expect(createButton()).toBeDisabled();
    await userEvent.type(name(), "a/b.md");
    expect(createButton()).toBeDisabled();
    await userEvent.clear(name());
    await userEvent.type(name(), "a\\b.md");
    expect(createButton()).toBeDisabled();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  // NTA-FR-05: extensionless names and dotfiles are as expressible as any other.
  it("NTA-FR-05: accepts an extensionless name and imposes no allowlist", async () => {
    const { onSubmit } = renderModal();
    await userEvent.selectOptions(type(), "instructions");
    await userEvent.type(name(), "Makefile");
    await userEvent.click(createButton());
    expect(onSubmit).toHaveBeenCalledWith({
      location: null,
      name: "Makefile",
      artifactType: "instructions",
    });
  });

  // NTA-FR-11 head / NTA-FR-10: one confirmation is one call, carrying the
  // three values the window holds.
  it("NTA-FR-10, NTA-FR-11: invokes the creation once with the three inputs", async () => {
    const { onSubmit } = renderModal();

    await userEvent.selectOptions(location(), "specifications");
    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    await userEvent.click(createButton());

    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith({
      location: "specifications",
      name: "overview.md",
      artifactType: "spec",
    });
  });

  // NTA-FR-10: a second confirmation is not accepted while the first is in
  // flight, so a double-click never issues two creations.
  it("NTA-FR-10: disables Create while a call is in flight", async () => {
    let release!: (r: SubmitResult) => void;
    const onSubmit = vi.fn<
      (p: NewTypedArtifactPayload) => Promise<SubmitResult>
    >(() => new Promise<SubmitResult>((resolve) => (release = resolve)));
    render(
      <NewTypedArtifactModal
        seed={{}}
        folders={FOLDERS}
        onClose={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    await userEvent.click(createButton());
    await waitFor(() => expect(createButton()).toBeDisabled());
    await userEvent.click(createButton());
    expect(onSubmit).toHaveBeenCalledTimes(1);
    release({ ok: true });
  });

  // NTA-FR-15: every dismissal route cancels, invoking nothing.
  it("NTA-FR-15: Escape, an outside click, Cancel and the close control all dismiss", async () => {
    for (const dismiss of [
      async () => userEvent.keyboard("{Escape}"),
      async () =>
        fireEvent.click(document.querySelector(".scrim") as HTMLElement),
      async () => userEvent.click(screen.getByRole("button", { name: "Cancel" })),
      async () => userEvent.click(screen.getByRole("button", { name: "Close" })),
    ]) {
      const { onClose, onSubmit } = renderModal();
      await userEvent.type(name(), "overview.md");
      await userEvent.selectOptions(type(), "spec");
      await dismiss();
      expect(onClose).toHaveBeenCalled();
      expect(onSubmit).not.toHaveBeenCalled();
      cleanup();
    }
  });

  // NTA-FR-10 / NTA-FR-16: a creation error renders inline, the window stays
  // open with every input as it was, and a corrected retry succeeds.
  it("NTA-FR-16, NTA-FR-10: shows a creation failure inline and keeps all three inputs", async () => {
    const onSubmit = vi
      .fn<(p: NewTypedArtifactPayload) => Promise<SubmitResult>>()
      .mockResolvedValueOnce({
        ok: false,
        error: "“overview.md” is already in specifications.",
      })
      .mockResolvedValueOnce({ ok: true });
    const onClose = vi.fn();
    render(
      <NewTypedArtifactModal
        seed={{ initialLocation: "specifications" }}
        folders={FOLDERS}
        onClose={onClose}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    await userEvent.click(createButton());

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("already in specifications");
    // The window stays open with Location, Name and Artifact Type unchanged.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(location()).toHaveValue("specifications");
    expect(name()).toHaveValue("overview.md");
    expect(type()).toHaveValue("spec");
    expect(onClose).not.toHaveBeenCalled();

    // And the corrected retry succeeds.
    await userEvent.clear(name());
    await userEvent.type(name(), "overview-2.md");
    await userEvent.click(createButton());
    expect(onSubmit).toHaveBeenCalledTimes(2);
    expect(onSubmit).toHaveBeenLastCalledWith({
      location: "specifications",
      name: "overview-2.md",
      artifactType: "spec",
    });
  });

  /**
   * NTA-FR-16, PST-FR-29: a failure raised *after* the file write — a refused type
   * assignment — differs from a collision in one way that matters: nothing of
   * the attempt survives it, so confirming again **unchanged** succeeds. The
   * window has nothing to correct and the author has nothing to redo.
   */
  it("NTA-FR-16, PST-FR-29: confirming again unchanged after a rolled-back failure succeeds", async () => {
    const onSubmit = vi
      .fn<(p: NewTypedArtifactPayload) => Promise<SubmitResult>>()
      .mockResolvedValueOnce({
        ok: false,
        error: "the artifact type could not be recorded for overview.md",
      })
      .mockResolvedValueOnce({ ok: true });
    const onClose = vi.fn();
    render(
      <NewTypedArtifactModal
        seed={{ initialLocation: "specifications" }}
        folders={FOLDERS}
        onClose={onClose}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    await userEvent.click(createButton());

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "could not be recorded",
    );
    // Nothing is touched between the two confirmations.
    expect(createButton()).toBeEnabled();
    await userEvent.click(createButton());

    expect(onSubmit).toHaveBeenCalledTimes(2);
    // Byte-identical: the retry meets the project exactly as the first attempt
    // found it, so it carries exactly what the first one carried.
    expect(onSubmit.mock.calls[1]).toEqual(onSubmit.mock.calls[0]);
    expect(onSubmit.mock.calls[1][0]).toEqual({
      location: "specifications",
      name: "overview.md",
      artifactType: "spec",
    });
  });

  // NTA-FR-09: Enter in the Name field is the keyboard route to Create, and it
  // is gated on exactly what the button is.
  it("NTA-FR-09: Enter submits only once the name and the type are both there", async () => {
    const { onSubmit } = renderModal();

    // No type yet: Enter does nothing.
    await userEvent.type(name(), "overview.md{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();

    await userEvent.selectOptions(type(), "spec");
    name().focus();
    await userEvent.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  // NTA-FR-09: a name that is only whitespace is no name at all.
  it("NTA-FR-09: a whitespace-only name leaves Create disabled", async () => {
    const { onSubmit } = renderModal();
    await userEvent.selectOptions(type(), "spec");
    await userEvent.type(name(), "   ");
    expect(createButton()).toBeDisabled();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  // NTA-FR-16: a rejected promise must still leave a usable window rather than
  // a Create button stuck disabled with nothing said.
  it("NTA-FR-16: reports a rejection inline and re-enables Create", async () => {
    const onSubmit = vi
      .fn<(p: NewTypedArtifactPayload) => Promise<SubmitResult>>()
      .mockRejectedValue(new Error("io failure"));
    render(
      <NewTypedArtifactModal
        seed={{}}
        folders={FOLDERS}
        onClose={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    await userEvent.click(createButton());

    expect(await screen.findByRole("alert")).toHaveTextContent("io failure");
    expect(createButton()).toBeEnabled();
  });

  // NTA-FR-15 / NTA NFR: focus lands in Name, Tab walks the frame without
  // leaving it, and dismissal returns focus to the invoking surface.
  it("NTA-FR-15: focuses Name, traps Tab in the frame, and restores focus on close", async () => {
    const invoker = document.createElement("button");
    invoker.textContent = "New Artifact";
    document.body.appendChild(invoker);
    invoker.focus();

    const { onClose } = renderModal();
    expect(name()).toHaveFocus();

    // Filled in first, so Create is enabled and therefore in the tab order at
    // all — a disabled button is not focusable, here or anywhere else.
    await userEvent.type(name(), "overview.md");
    await userEvent.selectOptions(type(), "spec");
    name().focus();

    // Tab reaches the other inputs and the two actions, and wraps rather than
    // walking on to whatever is behind the scrim.
    const reached: string[] = [];
    for (let i = 0; i < 6; i += 1) {
      await userEvent.tab();
      reached.push(
        (document.activeElement as HTMLElement)?.getAttribute("aria-label") ??
          document.activeElement?.textContent ??
          "",
      );
    }
    expect(reached.some((r) => r.startsWith("Location"))).toBe(true);
    expect(reached.some((r) => r.startsWith("Artifact Type"))).toBe(true);
    expect(reached.some((r) => r.includes("Cancel"))).toBe(true);
    expect(reached.some((r) => r.includes("Create"))).toBe(true);
    expect(document.activeElement).not.toBe(invoker);

    // NTA-FR-15: dismissal returns focus to the surface it was invoked from.
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    cleanup();
    await waitFor(() => expect(invoker).toHaveFocus());
    invoker.remove();
  });
});
