import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "./Settings";
import type { GithubBindingResolution, GithubTokenRecord } from "../types";

// SET-FR-12: the Project section names the token this project authenticates
// with and hands changing it to the picker modal the Git panel also opens.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const WORK: GithubTokenRecord = {
  id: "t1",
  label: "work laptop",
  accountLogin: "raver119",
  scopes: ["repo", "workflow"],
  maskedHint: "a3f9",
  addedAt: "2026-03-12T10:00:00Z",
  lastVerifiedAt: null,
  state: "valid",
};
const PERSONAL: GithubTokenRecord = {
  ...WORK,
  id: "t2",
  label: "personal",
  maskedHint: "1b04",
  scopes: ["repo"],
};

/**
 * Answer the section's two mount reads. `sequence` lets a test change the
 * answer between reads, which is how a rebind is observed.
 */
function backend(
  bindings: { tokenId: string | null; resolution: GithubBindingResolution }[],
  tokens: GithubTokenRecord[],
) {
  const queue = [...bindings];
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "get_project_github_token_binding")
      return queue.length > 1 ? queue.shift() : queue[0];
    if (cmd === "list_github_tokens") return tokens;
    if (cmd === "set_project_github_token_binding") return queue[0];
    return undefined;
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

afterEach(cleanup);

/**
 * Make sure the Project section is the one presented.
 *
 * SET-FR-03: it is the window's first section and the one it opens on, so this
 * is ordinarily already true — it is asked for by its nav row rather than by
 * its text because the section's own heading reads "Project" too.
 */
async function openProjectSection() {
  await userEvent.click(screen.getByRole("tab", { name: "Project" }));
}

describe("Project settings — GitHub token (SET-FR-12)", () => {
  it("SET-FR-12 names the bound token and describes it without ever showing more than the masked hint", async () => {
    backend([{ tokenId: "t1", resolution: "bound" }], [WORK, PERSONAL]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openProjectSection();

    await waitFor(() =>
      expect(screen.getByTestId("github-token-bound")).toBeInTheDocument(),
    );
    const card = screen.getByTestId("settings-github-token");
    expect(card).toHaveTextContent("work laptop");
    expect(card).toHaveTextContent("@raver119 · repo, workflow · ••••a3f9");
    // GHA-FR-03: four characters is the most of a token this tab ever renders.
    expect(card.textContent).not.toContain("ghp_");
  });

  it("SET-FR-12 opens the picker to change the binding and re-reads it afterwards", async () => {
    backend(
      [
        { tokenId: "t1", resolution: "bound" },
        { tokenId: "t2", resolution: "bound" },
      ],
      [WORK, PERSONAL],
    );
    // The picker is owned by the shell; this tab only asks for it and is told
    // when a choice has been persisted (GHA-FR-20).
    const onOpenGithubTokenPicker = vi.fn(
      (_currentTokenId: string | null, onConfirmed: () => void) => onConfirmed(),
    );
    render(
      <Settings
        contentRoot="/p#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
        onOpenGithubTokenPicker={onOpenGithubTokenPicker}
      />,
    );
    await openProjectSection();
    await waitFor(() =>
      expect(screen.getByTestId("github-token-bound")).toHaveTextContent(
        "work laptop",
      ),
    );

    await userEvent.click(screen.getByRole("button", { name: "Change…" }));

    // GHA-FR-20: the picker is told which token is currently bound, so it can
    // preselect it. Passing nothing here is what silently rebinds a project to
    // whichever token happens to sort first.
    expect(onOpenGithubTokenPicker).toHaveBeenCalledWith(
      "t1",
      expect.any(Function),
    );
    await waitFor(() =>
      expect(screen.getByTestId("github-token-bound")).toHaveTextContent(
        "personal",
      ),
    );
  });

  it("SET-FR-12 says where to add a token when none is stored, and offers nothing to change", async () => {
    // GHA-FR-19: with no token stored there is nothing to pick between.
    backend([{ tokenId: null, resolution: "none_stored" }], []);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openProjectSection();

    await waitFor(() =>
      expect(screen.getByTestId("github-token-none")).toHaveTextContent(
        /Global settings → GitHub/,
      ),
    );
    expect(screen.queryByRole("button", { name: "Change…" })).not.toBeInTheDocument();
  });

  it("says a lone token is used automatically rather than claiming a choice was made", async () => {
    // GHA-FR-18 / GTS-FR-10: `implicit` is not the same as `bound`, and the
    // section is honest about which one it is showing.
    backend([{ tokenId: "t1", resolution: "implicit" }], [WORK]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openProjectSection();

    await waitFor(() =>
      expect(screen.getByTestId("github-token-bound")).toHaveTextContent(
        /only token stored/,
      ),
    );
    // Changing it is still offered — a second token may be added later.
    expect(screen.getByRole("button", { name: "Change…" })).toBeInTheDocument();
  });

  it("reports an unbound project rather than silently showing nothing", async () => {
    backend([{ tokenId: null, resolution: "selection_required" }], [WORK, PERSONAL]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openProjectSection();

    await waitFor(() =>
      expect(screen.getByTestId("github-token-unbound")).toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Change…" })).toBeInTheDocument();
  });

  it("surfaces a failed read inline instead of rendering a blank card", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_project_github_token_binding") throw "keychain_unavailable";
      return [];
    });
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openProjectSection();

    await waitFor(() =>
      expect(
        screen.getByText(/system keychain is unavailable/),
      ).toBeInTheDocument(),
    );
  });
});
