import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubTokenPicker } from "./GithubTokenPicker";
import type { GithubTokenRecord } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function token(over: Partial<GithubTokenRecord> = {}): GithubTokenRecord {
  return {
    id: "t1",
    label: "work laptop",
    accountLogin: "raver119",
    scopes: ["repo", "workflow"],
    maskedHint: "a3f9",
    addedAt: "2026-03-12T10:00:00Z",
    lastVerifiedAt: null,
    state: "valid",
    ...over,
  };
}

const TWO = [token(), token({ id: "t2", label: "personal", maskedHint: "1b04" })];

function backend(
  tokens: GithubTokenRecord[],
  overrides: Record<string, (args?: Record<string, unknown>) => unknown> = {},
) {
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (overrides[cmd]) return overrides[cmd](args);
      if (cmd === "list_github_tokens") return tokens;
      if (cmd === "set_project_github_token_binding")
        return { tokenId: args?.tokenId, resolution: "bound" };
      throw new Error(`unexpected command ${cmd}`);
    },
  );
}

beforeEach(() => {
  // Braces matter: `mockReset()` returns the mock, and a value returned from a
  // Vitest hook is treated as a teardown callback — so the concise-arrow form
  // would have Vitest *call the mock* with no arguments after every test.
  invokeMock.mockReset();
});
afterEach(cleanup);

describe("GitHub token picker", () => {
  it("GHA-FR-15, GHA-FR-16 binds the chosen token and reports back so the caller can proceed", async () => {
    // GHA-FR-15 / GHA-FR-16: the operation that opened the picker runs once a
    // token has been bound.
    backend(TWO);
    const onConfirm = vi.fn();
    render(
      <GithubTokenPicker
        projectName="acme-platform"
        onCancel={vi.fn()}
        onConfirm={onConfirm}
      />,
    );

    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );
    // GHA-FR-15: the modal names the project it is choosing for, and says the
    // choice is remembered.
    expect(screen.getByRole("dialog")).toHaveTextContent("acme-platform");
    expect(screen.getByRole("dialog")).toHaveTextContent(/remembers this choice/);

    await userEvent.click(screen.getByRole("radio", { name: "personal" }));
    await userEvent.click(screen.getByRole("button", { name: "Use" }));

    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith("t2"));
    expect(invokeMock).toHaveBeenCalledWith("set_project_github_token_binding", {
      tokenId: "t2",
    });
  });

  it("GHA-FR-17 cancelling records no binding and abandons the operation", async () => {
    // GHA-FR-17.
    backend(TWO);
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <GithubTokenPicker
        projectName="acme-platform"
        onCancel={onCancel}
        onConfirm={onConfirm}
      />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onCancel).toHaveBeenCalled();
    expect(onConfirm).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith(
      "set_project_github_token_binding",
      expect.anything(),
    );
  });

  it("Escape and a backdrop click cancel too", async () => {
    // GHA-FR-17: every dismissal is a cancel, none of them binds anything.
    backend(TWO);
    const onCancel = vi.fn();
    const { container } = render(
      <GithubTokenPicker
        projectName="acme"
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    await userEvent.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledTimes(1);

    await userEvent.click(container.querySelector(".scrim")!);
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(invokeMock).not.toHaveBeenCalledWith(
      "set_project_github_token_binding",
      expect.anything(),
    );
  });

  it("GHA-FR-20 preselects the project's current token when opened to change it", async () => {
    // GHA-FR-20: opened from Project settings with nothing waiting on it.
    backend(TWO);
    render(
      <GithubTokenPicker
        projectName="acme"
        currentTokenId="t2"
        onCancel={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "personal" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    expect(screen.getByRole("radio", { name: "work laptop" })).toHaveAttribute(
      "aria-checked",
      "false",
    );
  });

  it("GHA-FR-22 offers no way to add, rename, or remove a token", async () => {
    // GHA-FR-22: it selects among what is stored and nothing else.
    backend(TWO);
    render(
      <GithubTokenPicker projectName="acme" onCancel={vi.fn()} onConfirm={vi.fn()} />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    for (const name of [/Add token/, /Rename/, /Remove/, /Verify/]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });

  it("renders each row with the same masked identity the settings list uses", async () => {
    // GHA-FR-03 / GHA-FR-15: one vocabulary, and four characters is still the
    // most of a token that is ever shown.
    backend(TWO);
    render(
      <GithubTokenPicker projectName="acme" onCancel={vi.fn()} onConfirm={vi.fn()} />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    const rows = screen.getAllByTestId("picker-token-row");
    expect(within(rows[0]).getByTestId("token-identity")).toHaveTextContent(
      "@raver119 · repo, workflow · ••••a3f9",
    );
    expect(screen.getByTestId("github-token-picker").textContent).not.toContain("ghp_");
  });

  it("is operable by keyboard alone", async () => {
    // GHA-NFR: the selection group is focusable and confirm works without a
    // pointer.
    backend(TWO);
    const onConfirm = vi.fn();
    render(
      <GithubTokenPicker projectName="acme" onCancel={vi.fn()} onConfirm={onConfirm} />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    const second = screen.getByRole("radio", { name: "personal" });
    second.focus();
    await userEvent.keyboard(" ");
    expect(second).toHaveAttribute("aria-checked", "true");

    await userEvent.click(screen.getByRole("button", { name: "Use" }));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith("t2"));
  });

  it("keeps the modal open and says why when the binding cannot be persisted", async () => {
    backend(TWO, {
      set_project_github_token_binding: () => {
        throw "unknown_token";
      },
    });
    const onConfirm = vi.fn();
    render(
      <GithubTokenPicker projectName="acme" onCancel={vi.fn()} onConfirm={onConfirm} />,
    );
    await waitFor(() =>
      expect(screen.getAllByTestId("picker-token-row")).toHaveLength(2),
    );

    await userEvent.click(screen.getByRole("button", { name: "Use" }));

    await waitFor(() =>
      expect(screen.getByTestId("picker-error")).toHaveTextContent(
        /no longer stored/,
      ),
    );
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByTestId("github-token-picker")).toBeInTheDocument();
  });
});
