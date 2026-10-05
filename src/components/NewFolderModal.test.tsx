import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  NewFolderModal,
  type NewFolderPayload,
  type SubmitResult,
} from "./NewFolderModal";
import { ARTIFACT_TYPES } from "../artifactTypes";
import type { FolderOption } from "../hooks/useProjectFolders";
import type { NewFolderSeed } from "../types";

// NFW-FR-04: every folder in the project is a candidate parent, including ones
// the Library's active lens hides — `drafts` is empty and `tooling` unclassified,
// so **All artifacts** would show neither, yet both must be selectable here.
const FOLDERS: FolderOption[] = [
  { path: "drafts", label: "drafts" },
  { path: "specifications", label: "specifications" },
  { path: "specifications/ui", label: "specifications/ui" },
  { path: "tooling", label: "tooling" },
];

function renderModal(seed: NewFolderSeed = {}, folders = FOLDERS) {
  const onClose = vi.fn();
  const onSubmit = vi.fn<(p: NewFolderPayload) => Promise<SubmitResult>>(
    async () => ({ ok: true }),
  );
  render(
    <NewFolderModal
      seed={seed}
      folders={folders}
      onClose={onClose}
      onSubmit={onSubmit}
    />,
  );
  return { onClose, onSubmit };
}

afterEach(() => cleanup());

describe("New Folder modal (NFW-new-folder.md)", () => {
  // NFW-FR-01, NFW-FR-02, NFW-FR-03: a modal overlay with exactly the three creation inputs — and
  // nothing a folder has no use for.
  it("NFW-FR-01, NFW-FR-02, NFW-FR-03: presents Parent Folder, Name and Artifact Type in a modal, with no body inputs", () => {
    renderModal();

    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    // The shared centered-overlay chrome, not a tab or a panel (NFW-FR-01).
    expect(dialog.closest(".scrim")).not.toBeNull();
    expect(screen.getByText("New Folder")).toBeInTheDocument();

    expect(screen.getByLabelText("Parent Folder")).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    expect(screen.getByLabelText("Artifact Type")).toBeInTheDocument();
    // A folder has no body: no Content, and no AI-prompt block (NFW-FR-03).
    expect(screen.queryByLabelText("Content")).not.toBeInTheDocument();
    expect(screen.queryByText("AI prompt")).not.toBeInTheDocument();
  });

  // NFW NFR: the window opens focused on the only value the user must supply.
  it("opens with keyboard focus on the Name input", () => {
    renderModal();
    expect(screen.getByLabelText("Name")).toHaveFocus();
  });

  // NFW-FR-04: the parent list is the project root plus every folder
  // — including the ones the active lens hides.
  it("NFW-FR-04: offers the project root and every folder, lens-hidden ones included", async () => {
    renderModal();

    const parent = screen.getByLabelText("Parent Folder") as HTMLSelectElement;
    const options = Array.from(parent.options).map((o) => o.value);
    expect(options).toEqual([
      "",
      "drafts",
      "specifications",
      "specifications/ui",
      "tooling",
    ]);
    expect(parent.options[0].text).toBe("(project root)");

    // And they are genuinely selectable, not merely listed.
    await userEvent.selectOptions(parent, "drafts");
    expect(parent).toHaveValue("drafts");
  });

  // NFW-FR-06: the File-menu flow's starting state.
  it("NFW-FR-06: the File-menu flow starts at the project root with an empty name and no type", () => {
    renderModal({});

    expect(screen.getByLabelText("Parent Folder")).toHaveValue("");
    expect(screen.getByLabelText("Name")).toHaveValue("");
    expect(screen.getByLabelText("Artifact Type")).toHaveValue("");
  });

  // NFW-FR-05 / NFW-FR-07: the context-menu flow seeds the parent and leaves it
  // editable; the type is still unset, never seeded from the folder's own type.
  it("NFW-FR-05, NFW-FR-07: the context-menu flow seeds an editable parent and still leaves the type unset", async () => {
    renderModal({ initialParent: "specifications" });

    const parent = screen.getByLabelText("Parent Folder");
    expect(parent).toHaveValue("specifications");
    expect(screen.getByLabelText("Artifact Type")).toHaveValue("");

    // A starting point rather than a commitment.
    await userEvent.selectOptions(parent, "tooling");
    expect(parent).toHaveValue("tooling");
  });

  // NFW-FR-04 / NFW-FR-07: a seeded parent the published list has not caught up
  // with is still offered, so the window never opens on a parent other than the
  // folder that was right-clicked.
  it("offers a seeded parent that is missing from the folder list", () => {
    renderModal({ initialParent: "brand-new" }, FOLDERS);

    const parent = screen.getByLabelText("Parent Folder") as HTMLSelectElement;
    expect(parent).toHaveValue("brand-new");
    expect(Array.from(parent.options).map((o) => o.value)).toContain("brand-new");
  });

  // NFW-FR-05: the type options are the explicit unset state followed by exactly
  // the eight built-in types (ASC-FR-02).
  it("NFW-FR-05: offers an unset state plus exactly the eight built-in types", () => {
    renderModal();

    const type = screen.getByLabelText("Artifact Type") as HTMLSelectElement;
    expect(Array.from(type.options).map((o) => o.value)).toEqual([
      "",
      ...ARTIFACT_TYPES.map((t) => t.value),
    ]);
    expect(type.options[0].text).toBe("(not set)");
    expect(type).toHaveValue("");
  });

  // NFW-FR-08: Create is gated on a bare-basename Name, so one
  // invocation can only ever create one folder.
  it("NFW-FR-08: disables Create until the Name is a non-empty bare basename", async () => {
    const { onSubmit } = renderModal();

    const create = screen.getByRole("button", { name: /Create/ });
    expect(create).toBeDisabled();

    // A path separator is not a name — it would be a chain of nested folders.
    await userEvent.type(screen.getByLabelText("Name"), "a/b");
    expect(create).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Name"), "{backspace}{backspace}");
    // "a" alone is valid.
    expect(create).toBeEnabled();

    // A backslash is rejected on the same terms.
    await userEvent.clear(screen.getByLabelText("Name"));
    await userEvent.type(screen.getByLabelText("Name"), "a\\b");
    expect(create).toBeDisabled();

    // Whitespace-only is empty.
    await userEvent.clear(screen.getByLabelText("Name"));
    await userEvent.type(screen.getByLabelText("Name"), "   ");
    expect(create).toBeDisabled();

    expect(onSubmit).not.toHaveBeenCalled();
  });

  // NFW-FR-10 / NFW-FR-09: Create invokes the operation with the current inputs.
  it("NFW-FR-09, NFW-FR-10: Create submits the parent, name and unset type", async () => {
    const { onSubmit } = renderModal({ initialParent: "specifications" });

    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    await userEvent.click(screen.getByRole("button", { name: /Create/ }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0]).toEqual({
      location: "specifications",
      name: "scenarios",
      artifactType: null,
    });
  });

  // NFW-FR-09 / NFW-FR-10: a chosen type rides along, and the project root is
  // submitted as an unset location.
  it("submits the chosen type, and an unset location for the project root", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    await userEvent.selectOptions(
      screen.getByLabelText("Artifact Type"),
      "scenario",
    );
    await userEvent.click(screen.getByRole("button", { name: /Create/ }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0]).toEqual({
      location: null,
      name: "scenarios",
      artifactType: "scenario",
    });
  });

  it("trims the submitted name", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "  scenarios  ");
    await userEvent.click(screen.getByRole("button", { name: /Create/ }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
    expect(onSubmit.mock.calls[0][0].name).toBe("scenarios");
  });

  it("submits on Enter in the Name field once the name is valid", async () => {
    const { onSubmit } = renderModal();

    const name = screen.getByLabelText("Name");
    // Enter with an invalid (empty) name does nothing.
    fireEvent.keyDown(name, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();

    await userEvent.type(name, "scenarios");
    fireEvent.keyDown(name, { key: "Enter" });
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
  });

  // NFW-FR-12: every dismissal route cancels without a backend call.
  it("NFW-FR-12: Escape, an outside click, and the close control each cancel", async () => {
    // Escape.
    let handles = renderModal();
    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    fireEvent.keyDown(window, { key: "Escape" });
    expect(handles.onClose).toHaveBeenCalledTimes(1);
    expect(handles.onSubmit).not.toHaveBeenCalled();
    cleanup();

    // A click on the backdrop, outside the modal frame.
    handles = renderModal();
    fireEvent.click(screen.getByRole("dialog").closest(".scrim")!);
    expect(handles.onClose).toHaveBeenCalledTimes(1);
    expect(handles.onSubmit).not.toHaveBeenCalled();
    cleanup();

    // The close control.
    handles = renderModal();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(handles.onClose).toHaveBeenCalledTimes(1);
    expect(handles.onSubmit).not.toHaveBeenCalled();
  });

  it("NFW-FR-12: a click inside the modal frame does not dismiss it", async () => {
    const { onClose } = renderModal();
    await userEvent.click(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("Cancel dismisses without creating anything", async () => {
    const { onClose, onSubmit } = renderModal();
    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  // NFW-FR-13: a failure is shown inline and the window stays open
  // with its inputs intact, so the user can adjust the name and retry.
  it("NFW-FR-13: shows a creation failure inline and keeps the inputs", async () => {
    const onClose = vi.fn();
    const onSubmit = vi.fn<(p: NewFolderPayload) => Promise<SubmitResult>>(
      async () => ({ ok: false, error: "already exists: specifications/scenarios" }),
    );
    render(
      <NewFolderModal
        seed={{ initialParent: "specifications" }}
        folders={FOLDERS}
        onClose={onClose}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    await userEvent.selectOptions(screen.getByLabelText("Artifact Type"), "spec");
    await userEvent.click(screen.getByRole("button", { name: /Create/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("already exists");
    expect(onClose).not.toHaveBeenCalled();
    // Inputs intact — including the type, so a retry does not silently drop it.
    expect(screen.getByLabelText("Parent Folder")).toHaveValue("specifications");
    expect(screen.getByLabelText("Name")).toHaveValue("scenarios");
    expect(screen.getByLabelText("Artifact Type")).toHaveValue("spec");
    // And Create is usable again rather than stuck disabled after the failure.
    expect(screen.getByRole("button", { name: /Create/ })).toBeEnabled();
  });

  it("surfaces a rejected onSubmit instead of wedging the Create button", async () => {
    // `onSubmit` is contracted to resolve a result, but a rejection must still
    // leave a usable window — not a Create button disabled forever with nothing
    // said about why (NFW-FR-13).
    const onSubmit = vi.fn<(p: NewFolderPayload) => Promise<SubmitResult>>(
      async () => {
        throw "boom";
      },
    );
    render(
      <NewFolderModal
        seed={{}}
        folders={FOLDERS}
        onClose={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    await userEvent.click(screen.getByRole("button", { name: /Create/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent("boom");
    expect(screen.getByRole("button", { name: /Create/ })).toBeEnabled();
  });

  it("does not fire a second creation while one is in flight", async () => {
    let release: (r: SubmitResult) => void = () => {};
    const onSubmit = vi.fn<(p: NewFolderPayload) => Promise<SubmitResult>>(
      () => new Promise<SubmitResult>((resolve) => (release = resolve)),
    );
    render(
      <NewFolderModal
        seed={{}}
        folders={FOLDERS}
        onClose={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(screen.getByLabelText("Name"), "scenarios");
    const create = screen.getByRole("button", { name: /Create/ });
    await userEvent.click(create);
    await waitFor(() => expect(create).toBeDisabled());
    fireEvent.click(create);
    expect(onSubmit).toHaveBeenCalledTimes(1);

    release({ ok: true });
  });
});
