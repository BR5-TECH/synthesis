/**
 * CHG-FR-59: the blocking rollback confirmation.
 *
 * What is under test is that the surface says enough to be answered — the count,
 * every path, both identities of a rename, and the four things that get
 * discarded — and that every route out of it that is not the destructive action
 * performs nothing at all.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { RollbackConfirm, rollbackFateOf } from "./RollbackConfirm";

// The suite does not auto-unmount between tests, so a second render would find
// two dialogs and every role query would be ambiguous.
afterEach(cleanup);

const FILES = [
  { path: ".claude/skills/onboarding.md", untracked: false },
  { path: "src/components/Changes.tsx", untracked: true },
  {
    path: "src-tauri/lib.rs",
    previousPath: "src-tauri/main.rs",
    untracked: false,
  },
];

describe("the rollback confirmation (CHG-FR-59)", () => {
  it("names the count and every affected path", () => {
    render(<RollbackConfirm files={FILES} onSettle={vi.fn()} />);

    expect(
      screen.getByRole("heading", { name: "Discard 3 files?" }),
    ).toBeInTheDocument();
    for (const f of FILES) {
      expect(screen.getByText(f.path)).toBeInTheDocument();
    }
    // GTC-FR-26: a rename acts on both identities, so both are named.
    expect(screen.getByText("(was src-tauri/main.rs)")).toBeInTheDocument();
    // An untracked file is deleted rather than restored, and says so.
    expect(screen.getByText("(deleted)")).toBeInTheDocument();
  });

  it("warns about all four things that are discarded", () => {
    render(<RollbackConfirm files={FILES} onSettle={vi.fn()} />);
    const warning = screen.getByText(/tracked and staged changes/i);
    expect(warning.textContent).toMatch(/tracked and staged changes/i);
    expect(warning.textContent).toMatch(/untracked files are deleted/i);
    expect(warning.textContent).toMatch(/unsaved Editor and Diff edits/i);
    expect(warning.textContent).toMatch(/cannot be undone/i);
  });

  it("agrees in number for a single file", () => {
    render(<RollbackConfirm files={[FILES[0]]} onSettle={vi.fn()} />);
    expect(
      screen.getByRole("heading", { name: "Discard 1 file?" }),
    ).toBeInTheDocument();
  });

  it("performs nothing on Cancel, Escape, or an outside pointer-down", async () => {
    // Each dismissal route reports `false`, which is what makes it a no-op all
    // the way up: the panel never reaches the backend for it.
    for (const dismiss of ["cancel", "escape", "outside"] as const) {
      const onSettle = vi.fn();
      const { container, unmount } = render(
        <RollbackConfirm files={FILES} onSettle={onSettle} />,
      );
      if (dismiss === "cancel") {
        await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
      } else if (dismiss === "escape") {
        await userEvent.keyboard("{Escape}");
      } else {
        await userEvent.click(container.querySelector(".scrim")!);
      }
      expect(onSettle).toHaveBeenCalledWith(false);
      unmount();
    }
  });

  it("confirms only through the destructive action", async () => {
    const onSettle = vi.fn();
    render(<RollbackConfirm files={FILES} onSettle={onSettle} />);
    await userEvent.click(
      screen.getByRole("button", { name: "Discard 3 files" }),
    );
    expect(onSettle).toHaveBeenCalledWith(true);
  });

  it("puts initial focus on Cancel rather than the destructive action", () => {
    render(<RollbackConfirm files={FILES} onSettle={vi.fn()} />);
    // A reflexive Enter on a modal the author did not expect must discard
    // nothing.
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("is a modal dialog described by its warning", () => {
    render(<RollbackConfirm files={FILES} onSettle={vi.fn()} />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleDescription(/cannot be undone/i);
  });
});

describe("rollbackFateOf", () => {
  it("describes what will happen to each kind of entry", () => {
    expect(rollbackFateOf({ path: "a", untracked: true })).toBe("deleted");
    expect(
      rollbackFateOf({ path: "a", untracked: false, previousPath: "b" }),
    ).toBe("was b");
    expect(rollbackFateOf({ path: "a", untracked: false, deleted: true })).toBe(
      "restored",
    );
    // An ordinary modification needs no annotation: the warning above already
    // says what happens to it.
    expect(rollbackFateOf({ path: "a", untracked: false })).toBeNull();
  });
});
