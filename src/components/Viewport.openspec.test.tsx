/**
 * The viewport routes a change workspace tab to its surface
 * (`../../specifications/ui/TAB-tabs.md` TAB-FR-02), and mounts it on each
 * activation (`../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-LABL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { resetLogBufferForTest } from "../logging";
import { resetChangeViewsForTest } from "../state/openspec/changeView";
import type { Tab } from "../types";
import { demoChange } from "./OpenSpecChange/fixtures";
import { Viewport } from "./Viewport";

beforeEach(() => {
  resetLogBufferForTest();
  resetChangeViewsForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "read_openspec_change")
      return demoChange(changeOf((args as { name: string }).name));
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

describe("a change workspace tab in the viewport", () => {
  it("TAB-FR-02: an OpenSpec change is tabbable and opens in the change workspace", async () => {
    const tab: Tab = {
      id: "openspec:stream:s-1:add-logout-3f2a",
      label: "add-logout-3f2a",
      kind: "openspecChange",
      openspecChange: {
        location: { kind: "stream", streamId: "s-1" },
        name: "add-logout-3f2a",
        streamName: "auth-work",
        file: null,
        openSeq: 1,
      },
    };
    const props = {
      activeTab: tab.id,
      activeT: tab,
      onOpenArtifact: vi.fn(),
    } as unknown as ComponentProps<typeof Viewport>;
    render(<Viewport {...props} />);
    expect(screen.getByTestId("openspec-change")).toBeInTheDocument();
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((c) => c[0] === "read_openspec_change")).toHaveLength(1),
    );
    expect(await screen.findByTestId("openspec-reading")).toBeInTheDocument();
  });
});

const B_NAME = "rename-login-7c1d";

/** The read of B differs from the read of A in more than the name. */
function changeOf(name: string) {
  return name === B_NAME ? { name, completedTasks: 3, schema: "minimal" } : { name };
}

const changeTab = (name: string): Tab => ({
  id: `openspec:stream:s-1:${name}`,
  label: name,
  kind: "openspecChange",
  openspecChange: {
    location: { kind: "stream", streamId: "s-1" },
    name,
    streamName: "auth-work",
    file: null,
    openSeq: 1,
  },
});

const readsOf = (name: string) =>
  invokeMock.mock.calls.filter(
    (c) => c[0] === "read_openspec_change" && (c[1] as { name: string }).name === name,
  );

function viewportProps(tab: Tab) {
  return { activeTab: tab.id, activeT: tab, onOpenArtifact: vi.fn() } as unknown as ComponentProps<
    typeof Viewport
  >;
}

/** Wait until the change on show has its read and no read is in progress. */
async function settledOn(name: string) {
  expect(await screen.findByRole("heading", { name })).toBeInTheDocument();
  await screen.findByTestId("openspec-reading");
  await waitFor(() => expect(screen.getByRole("button", { name: "Refresh" })).not.toBeDisabled());
}

describe("a change workspace tab that becomes the active tab again", () => {
  it("OCW-FR-LABL: reads again on each activation, and reads nothing while another tab is active", async () => {
    const a = changeTab("add-logout-3f2a");
    const b = changeTab(B_NAME);
    const view = render(<Viewport {...viewportProps(a)} />);
    await settledOn("add-logout-3f2a");
    expect(readsOf("add-logout-3f2a")).toHaveLength(1);

    // B is a new mount: it has no read of its own yet, so it shows the
    // loading state and none of the content of A.
    view.rerender(<Viewport {...viewportProps(b)} />);
    expect(screen.getByTestId("openspec-change-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("openspec-change-tasks")).toBeNull();
    await settledOn(B_NAME);
    expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("3 of 4 tasks");
    expect(screen.getByText("minimal")).toBeInTheDocument();
    expect(readsOf(B_NAME)).toHaveLength(1);
    // A Refresh of the active tab reads that tab alone.
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(readsOf(B_NAME)).toHaveLength(2));
    await settledOn(B_NAME);
    expect(readsOf("add-logout-3f2a")).toHaveLength(1);

    // A shows its own kept read at once, and reads again.
    view.rerender(<Viewport {...viewportProps(a)} />);
    expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("1 of 4 tasks");
    await settledOn("add-logout-3f2a");
    await waitFor(() => expect(readsOf("add-logout-3f2a")).toHaveLength(2));
    expect(readsOf(B_NAME)).toHaveLength(2);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "read_openspec_change")).toHaveLength(4);
  });
});
