import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";

/**
 * `ACT-action-control.md` ACT-FR-17 when the project stores no GitHub token:
 * Discuss posts as the fixed local participant, and a required token binding
 * still keeps the composer disabled.
 */
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

interface Backend {
  identity?: { kind: "human"; login: string; displayName?: string };
  identityError?: string;
  calls: { cmd: string; args: unknown }[];
}

const backend = (over: Partial<Backend> = {}): Backend => ({
  calls: [],
  ...over,
});

const calls = (b: Backend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

async function renderEditor(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: "# Editor\n", checksum: "ck1" };
      case "list_discussions":
      case "list_ai_api_catalogs":
      case "list_agent_turns":
        return [];
      case "list_project_agents":
        return [
          { agent: { id: "a1", nickname: "arch" }, availability: "ready" },
        ];
      case "get_active_ai_api_catalog":
        return { resolution: "none_configured", catalog: null };
      case "resolve_comment_author_identity":
        if (b.identityError) throw b.identityError;
        return b.identity;
      case "open_discussion":
        return {
          id: "d-new",
          target: { kind: "artifact", artifactId: "a.md" },
          fragmentTarget: null,
          comments: [],
          locked: false,
          resolved: false,
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        };
      case "load_project_tree":
        return { id: "", name: "p", path: "", nodeKind: "folder", children: [] };
      default:
        return undefined;
    }
  });
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByRole("button", { name: "Actions" });
}

async function openActions() {
  await userEvent.click(screen.getByRole("button", { name: "Actions" }));
  return screen.getByRole("menu");
}

beforeEach(() => {
  invokeMock.mockReset();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
});
afterEach(cleanup);

describe("Discuss without a stored GitHub token (ACT-FR-17)", () => {
  it("ACT-FR-17, ACT-FR-RVCU: a project without a token posts as Me, with no token reason and no author supplied", async () => {
    const b = backend({ identity: { kind: "human", login: "", displayName: "Me" } });
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    await waitFor(() => expect(composer).toBeEnabled());
    expect(screen.queryByText(/GitHub account|Global settings|Choose a token/i)).toBeNull();

    await userEvent.type(composer, "is this ready?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(calls(b, "open_discussion")).toHaveLength(1),
    );
    const [opened] = calls(b, "open_discussion") as [Record<string, unknown>];
    expect(Object.keys(opened).sort()).toEqual([
      "attachments",
      "body",
      "fragmentTarget",
      "target",
    ]);
  });

  it("ACT-FR-17, CMT-FR-25: a required token binding keeps the composer disabled, and Me is no fallback", async () => {
    const b = backend({ identityError: "github_token_selection_required" });
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    expect(composer).toBeDisabled();
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });
});
