import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetSettingsSections } from "../state/settingsSweep";
import { otherCommand } from "../test/invokeMocks";
import { Settings } from "../test/settingsHarness";

// SET-FR-06 / MSS-FR-DZIQ: the MCP servers section of Project settings.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetSettingsSections();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_mcp_servers") return Promise.resolve([]);
    if (cmd === "load_project_config") return Promise.resolve({ lineEndings: "lf", draftTemplate: null });
    if (cmd === "get_project_github_token_binding") return Promise.resolve(null);
    if (cmd === "list_github_tokens") return Promise.resolve([]);
    return otherCommand(cmd);
  });
});
afterEach(cleanup);

describe("Project settings — MCP servers section", () => {
  it("SET-FR-06 / MSS-FR-DZIQ: the heading is followed by the sentence of the section, and the section is mounted", async () => {
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await userEvent.click(screen.getByRole("tab", { name: "MCP servers" }));
    const heading = screen.getByRole("heading", { name: "MCP servers" });
    expect(heading.nextElementSibling).toHaveTextContent(
      "The MCP servers whose tools the agents of this project can use.",
    );
    expect(await screen.findByTestId("mcp-servers-empty")).toBeInTheDocument();
    // The stub of the sections that are not built is gone.
    expect(screen.queryByText("Section forms are stubbed in this UI kit.")).toBeNull();
  });
});
