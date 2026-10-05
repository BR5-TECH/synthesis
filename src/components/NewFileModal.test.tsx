import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  NewFileModal,
  type NewFilePayload,
  type SubmitResult,
} from "./NewFileModal";
import type { FolderOption } from "../hooks/useProjectFolders";
import type { NewFileSeed } from "../types";

// NFI-FR-04: every folder in the project is a candidate location, including ones
// the Library's active lens hides — `drafts` is empty and `tooling` unclassified,
// so **All artifacts** would show neither, yet both must be selectable here.
const FOLDERS: FolderOption[] = [
  { path: "drafts", label: "drafts" },
  { path: "specifications", label: "specifications" },
  { path: "src/hooks", label: "src/hooks" },
  { path: "tooling", label: "tooling" },
];

function renderModal(seed: NewFileSeed = {}, folders = FOLDERS) {
  const onClose = vi.fn();
  const onSubmit = vi.fn<(p: NewFilePayload) => Promise<SubmitResult>>(
    async () => ({ ok: true }),
  );
  render(
    <NewFileModal
      seed={seed}
      folders={folders}
      onClose={onClose}
      onSubmit={onSubmit}
    />,
  );
  return { onClose, onSubmit };
}

afterEach(() => cleanup());

describe("New File modal (NFI-new-file.md)", () => {
  // NFI-FR-01, NFI-FR-02, NFI-FR-03: a modal overlay with exactly two inputs — and nothing a plain file
  // has no use for.
  it("NFI-FR-01, NFI-FR-02, NFI-FR-03: presents Location and Name in a modal, with no type, content or AI-prompt input", () => {
    renderModal();

    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    // The shared centered-overlay chrome, not a tab or a panel (NFI-FR-01).
    expect(dialog.closest(".scrim")).not.toBeNull();
    expect(screen.getByText("New File")).toBeInTheDocument();

    expect(screen.getByLabelText("Location")).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
    // NFI-FR-03: a regular file is named and placed here, not typed or authored.
    expect(screen.queryByLabelText("Artifact Type")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Type")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Content")).not.toBeInTheDocument();
    expect(screen.queryByText("AI prompt")).not.toBeInTheDocument();
  });

  // NFI-FR-04: the select offers every folder that exists, lens-hidden included.
  it("NFI-FR-04: offers every project folder as a location, including ones the active lens hides", () => {
    renderModal();

    const select = screen.getByLabelText("Location") as HTMLSelectElement;
    const options = Array.from(select.options).map((o) => o.value);
    // The project root plus all four folders — `drafts` (empty) and `tooling`
    // (unclassified) among them.
    expect(options).toEqual([
      "",
      "drafts",
      "specifications",
      "src/hooks",
      "tooling",
    ]);
  });

  // NFI-FR-04, NFI-FR-06: the File-menu flow. Location at the project root, Name empty, and
  // the location still the user's to change (NFI-FR-04 / NFI-FR-06).
  it("NFI-FR-04, NFI-FR-06: the File-menu flow starts at the project root with an empty name and an editable location", async () => {
    const { onSubmit } = renderModal({});

    const location = screen.getByLabelText("Location") as HTMLSelectElement;
    expect(location.value).toBe("");
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("");

    // Editable: picking another folder sticks and is what gets submitted.
    fireEvent.change(location, { target: { value: "src/hooks" } });
    expect(location.value).toBe("src/hooks");
    await userEvent.type(screen.getByLabelText("Name"), "helpers.ts");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        location: "src/hooks",
        name: "helpers.ts",
      }),
    );
  });

  // NFI-FR-07: the Library-context-menu flow. The right-clicked folder is a
  // starting point rather than a commitment (NFI-FR-07).
  it("NFI-FR-07: the context-menu flow seeds the location and leaves it editable", async () => {
    const { onSubmit } = renderModal({ initialLocation: "src/hooks" });

    const location = screen.getByLabelText("Location") as HTMLSelectElement;
    expect(location.value).toBe("src/hooks");
    expect(location.disabled).toBe(false);

    // Changed to a different folder before confirming.
    fireEvent.change(location, { target: { value: "drafts" } });
    await userEvent.type(screen.getByLabelText("Name"), "notes.md");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        location: "drafts",
        name: "notes.md",
      }),
    );
  });

  // A seeded folder the published list has not caught up with is still offered,
  // so a context-menu invocation never opens on a location other than the folder
  // that was right-clicked (NFI-FR-07).
  it("offers a seeded location the published folder list does not carry yet", () => {
    renderModal({ initialLocation: "brand/new" });

    const select = screen.getByLabelText("Location") as HTMLSelectElement;
    expect(select.value).toBe("brand/new");
    expect(Array.from(select.options).map((o) => o.value)).toContain("brand/new");
  });

  // NFI-FR-05: the Name is the complete basename. No extension is required, and
  // nothing is appended to what was typed (NFI-FR-05).
  it("NFI-FR-05: accepts an extensionless name and a dotfile verbatim", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "Makefile");
    expect((screen.getByText("Create").closest("button") as HTMLButtonElement).disabled).toBe(
      false,
    );
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      // Exactly what was typed, at the project root.
      expect(onSubmit).toHaveBeenCalledWith({ location: null, name: "Makefile" }),
    );
  });

  it("NFI-FR-05: accepts a name whose whole value is its extension", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), ".gitignore");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({ location: null, name: ".gitignore" }),
    );
  });

  it("NFI-FR-05: passes a multi-dot extension through untouched", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "notes.tar.gz");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        location: null,
        name: "notes.tar.gz",
      }),
    );
  });

  // NFI-FR-08: Create is gated on a valid name, so no call is ever made with a
  // malformed one (NFI-FR-08).
  it("NFI-FR-08: disables Create while the name is empty or holds a path separator", async () => {
    const { onSubmit } = renderModal();

    const create = screen.getByText("Create").closest("button") as HTMLButtonElement;
    const name = screen.getByLabelText("Name");
    // Empty.
    expect(create.disabled).toBe(true);

    // A forward slash: one invocation must not create the folders on the way.
    await userEvent.type(name, "a/b.ts");
    expect(create.disabled).toBe(true);

    // A backslash, likewise.
    await userEvent.clear(name);
    await userEvent.type(name, "a\\b.ts");
    expect(create.disabled).toBe(true);

    // Whitespace only is empty.
    await userEvent.clear(name);
    await userEvent.type(name, "   ");
    expect(create.disabled).toBe(true);

    // Nothing was submitted along the way…
    expect(onSubmit).not.toHaveBeenCalled();

    // …and a valid name enables it.
    await userEvent.clear(name);
    await userEvent.type(name, "helpers.ts");
    expect(create.disabled).toBe(false);
  });

  it("NFI-FR-08: Enter does not submit while the name is invalid", async () => {
    const { onSubmit } = renderModal();

    const name = screen.getByLabelText("Name");
    await userEvent.type(name, "a/b.ts");
    fireEvent.keyDown(name, { key: "Enter" });
    expect(onSubmit).not.toHaveBeenCalled();

    await userEvent.clear(name);
    await userEvent.type(name, "ok.ts");
    fireEvent.keyDown(name, { key: "Enter" });
    await waitFor(() => expect(onSubmit).toHaveBeenCalledTimes(1));
  });

  // NFI-FR-09, NFI-FR-10: confirming submits the current inputs; an unset location is null,
  // which the backend resolves to the project root (NFI-FR-09).
  it("NFI-FR-09, NFI-FR-10: submits an unset location as null", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "package.json");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        location: null,
        name: "package.json",
      }),
    );
  });

  it("trims surrounding whitespace off the submitted name", async () => {
    const { onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "  helpers.ts  ");
    fireEvent.click(screen.getByText("Create"));
    await waitFor(() =>
      expect(onSubmit).toHaveBeenCalledWith({
        location: null,
        name: "helpers.ts",
      }),
    );
  });

  // NFI-FR-13: every dismissal path cancels without invoking anything.
  it("NFI-FR-13: Escape dismisses without creating anything", async () => {
    const { onClose, onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "helpers.ts");
    fireEvent.keyDown(window, { key: "Escape" });

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("NFI-FR-13: an outside click dismisses without creating anything", async () => {
    const { onClose, onSubmit } = renderModal();

    await userEvent.type(screen.getByLabelText("Name"), "helpers.ts");
    fireEvent.click(screen.getByRole("dialog").closest(".scrim")!);

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("NFI-FR-13: a click inside the frame does not dismiss", async () => {
    const { onClose } = renderModal();

    fireEvent.click(screen.getByRole("dialog"));

    expect(onClose).not.toHaveBeenCalled();
  });

  it("NFI-FR-13: the close control and Cancel both dismiss without creating", () => {
    const { onClose, onSubmit } = renderModal();

    fireEvent.click(screen.getByLabelText("Close"));
    fireEvent.click(screen.getByText("Cancel"));

    expect(onClose).toHaveBeenCalledTimes(2);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  // NFI-FR-14: a creation failure keeps the window open with its inputs intact
  // and shows the error inline (NFI-FR-14).
  it("NFI-FR-14: shows a collision error inline and keeps the inputs intact", async () => {
    const onClose = vi.fn();
    const onSubmit = vi.fn<(p: NewFilePayload) => Promise<SubmitResult>>(
      async () => ({ ok: false, error: "already exists: src/hooks/helpers.ts" }),
    );
    render(
      <NewFileModal
        seed={{ initialLocation: "src/hooks" }}
        folders={FOLDERS}
        onClose={onClose}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(screen.getByLabelText("Name"), "helpers.ts");
    fireEvent.click(screen.getByText("Create"));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "already exists: src/hooks/helpers.ts",
      ),
    );
    // Still open, inputs untouched, and dismissal was never requested.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe(
      "helpers.ts",
    );
    expect((screen.getByLabelText("Location") as HTMLSelectElement).value).toBe(
      "src/hooks",
    );
    expect(onClose).not.toHaveBeenCalled();
    // Create is usable again, so the user can correct the name and retry.
    expect(
      (screen.getByText("Create").closest("button") as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  // A rejected `onSubmit` must not leave Create stuck disabled with nothing said
  // (the defensive half of NFI-FR-14).
  it("surfaces a rejected submission rather than wedging the Create button", async () => {
    const onSubmit = vi.fn<(p: NewFilePayload) => Promise<SubmitResult>>(
      async () => {
        throw new Error("backend unreachable");
      },
    );
    render(
      <NewFileModal
        seed={{}}
        folders={FOLDERS}
        onClose={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    await userEvent.type(screen.getByLabelText("Name"), "helpers.ts");
    fireEvent.click(screen.getByText("Create"));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("backend unreachable"),
    );
    expect(
      (screen.getByText("Create").closest("button") as HTMLButtonElement).disabled,
    ).toBe(false);
  });

  // The window opens with focus on the Name input, the only value the user must
  // always supply (NFI NFR).
  it("focuses the Name input on open", () => {
    renderModal();

    expect(document.activeElement).toBe(screen.getByLabelText("Name"));
  });
});
