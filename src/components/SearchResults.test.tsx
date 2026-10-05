import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { SearchResults } from "./SearchResults";
import { SearchSessionStore } from "../state/searchSessions";
import {
  createEventBus,
  emitEnded,
  emitResults,
  hit,
  resetHitOrdinals,
} from "../test/searchEvents";

// The full results page (SCH-FR-08) dispatches its OWN `scope = full` search
// and streams into itself, so it needs the same drivable event bus the overlay
// scenarios use.
const bus = createEventBus();
let searchIds: string[] = [];
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));

const lastSearchId = () => searchIds[searchIds.length - 1];
const startCalls = () => invokeMock.mock.calls.filter((c) => c[0] === "start_search");

beforeEach(() => {
  bus.reset();
  searchIds = [];
  resetHitOrdinals();
  let counter = 0;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "start_search") {
      const id = `full-${counter++}`;
      searchIds.push(id);
      return id;
    }
    return undefined;
  });
});

afterEach(cleanup);

const TARGET = { query: "needle", mode: "literal_insensitive" as const };

describe("Search results tab (SCH-FR-08 / SCH-FR-18)", () => {
  it("SCH-FR-08: dispatches its own search with scope = full", async () => {
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );

    await waitFor(() => expect(startCalls()).toHaveLength(1));
    expect(startCalls()[0][1]).toEqual({
      query: "needle",
      mode: "literal_insensitive",
      scope: "full",
    });
  });

  it("streams hits into the page as batches arrive", async () => {
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));

    act(() => emitResults(bus, lastSearchId(), [
      hit({ path: "src/a.rs", group: "file" }),
      hit({ path: "specs/b.spec.md", group: "artifact", subtype: "spec" }),
    ]));

    expect(await screen.findByText("a.rs")).toBeInTheDocument();
    expect(screen.getByText("b.spec.md")).toBeInTheDocument();
  });

  it("SCH-FR-09: activating a result on the page routes it", async () => {
    const onActivate = vi.fn();
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={onActivate}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() => emitResults(bus, lastSearchId(), [hit({ path: "src/main.rs", group: "file" })]));

    await userEvent.click(await screen.findByText("main.rs"));

    expect(onActivate).toHaveBeenCalledWith(
      expect.objectContaining({ id: "src/main.rs" }),
    );
  });
});

describe("full results filters (SCH-FR-10)", () => {
  it("SCH-FR-10: the page filters the result set; the overlay offers no such control", async () => {
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() => emitResults(bus, lastSearchId(), [
      hit({ path: "specs/a.spec.md", group: "artifact", subtype: "spec" }),
      hit({ path: "skills/b.skill.md", group: "artifact", subtype: "skill" }),
      hit({ path: "src/main.rs", group: "file" }),
    ]));
    act(() => emitEnded(bus, lastSearchId()));
    await screen.findByText("a.spec.md");

    const filter = screen.getByRole("combobox", { name: /Filter by artifact type/i });
    await userEvent.selectOptions(filter, "spec");

    expect(screen.getByText("a.spec.md")).toBeInTheDocument();
    expect(screen.queryByText("b.skill.md")).not.toBeInTheDocument();
    expect(screen.queryByText("main.rs")).not.toBeInTheDocument();

    // "All artifacts" excludes unclassified files; "All files" restores them.
    await userEvent.selectOptions(filter, "artifacts");
    expect(screen.getByText("b.skill.md")).toBeInTheDocument();
    expect(screen.queryByText("main.rs")).not.toBeInTheDocument();
    await userEvent.selectOptions(filter, "files");
    expect(screen.getByText("main.rs")).toBeInTheDocument();
  });

  it("SCH-FR-10: group counts move as the filter narrows the set", async () => {
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() =>
      emitResults(bus, lastSearchId(), [
        hit({ path: "specs/a.spec.md", group: "artifact", subtype: "spec" }),
        hit({ path: "specs/b.spec.md", group: "artifact", subtype: "spec" }),
        hit({ path: "src/main.rs", group: "file" }),
      ]),
    );
    act(() => emitEnded(bus, lastSearchId()));
    await screen.findByText("a.spec.md");

    const countOf = (title: string) =>
      screen
        .getByRole("button", { name: new RegExp(title) })
        .querySelector(".search-group__count")?.textContent;
    expect(countOf("Artifacts")).toBe("2");
    expect(countOf("Files")).toBe("1");

    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: /Filter by artifact type/i }),
      "artifacts",
    );

    expect(countOf("Artifacts")).toBe("2");
    // SCH-FR-04: the Files group is gone entirely once it has no hits.
    expect(screen.queryByRole("button", { name: /Files/ })).toBeNull();
  });
});

describe("tab preservation (SCH-FR-11)", () => {
  it("SCH-FR-11: returning to the tab shows the same results without re-dispatching", async () => {
    const sessions = new SearchSessionStore();
    const { unmount } = render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() => emitResults(bus, lastSearchId(), [hit({ path: "src/main.rs", group: "file" })]));
    act(() => emitEnded(bus, lastSearchId()));
    await screen.findByText("main.rs");
    expect(startCalls()).toHaveLength(1);

    // Navigating to the tab the result opened unmounts this one (the viewport
    // renders only the focused tab); returning re-mounts it.
    unmount();
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );

    expect(await screen.findByText("main.rs")).toBeInTheDocument();
    expect(startCalls()).toHaveLength(1);
    // And it is not stuck claiming to still be searching.
    expect(screen.queryByText(/Searching/i)).not.toBeInTheDocument();
  });

  it("a tab abandoned mid-sweep re-dispatches rather than resuming a partial answer", async () => {
    const sessions = new SearchSessionStore();
    const { unmount } = render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    // One batch, but no terminal event: the sweep never finished.
    act(() => emitResults(bus, lastSearchId(), [hit({ path: "src/main.rs", group: "file" })]));
    await screen.findByText("main.rs");

    unmount();
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );

    // A partial answer resumed as a finished one would leave the page showing
    // results for a search that was cancelled, and never completing.
    await waitFor(() => expect(startCalls()).toHaveLength(2));
  });

  it("SCH-FR-18, SCH-FR-20: a sweep superseded by the overlay retries instead of settling truncated", async () => {
    // At most one search runs at a time (SCC-FR-12), so typing in the overlay
    // supersedes this tab's full sweep — while SCH-FR-20 requires the sweep to
    // survive the overlay. Settling on the partial set would strand the tab on
    // a truncated answer it would never ask for again.
    const sessions = new SearchSessionStore();
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() =>
      emitResults(bus, lastSearchId(), [
        hit({ path: "src/partial.rs", group: "file" }),
      ]),
    );
    await screen.findByText("partial.rs");

    act(() => emitEnded(bus, lastSearchId(), "superseded"));

    // It re-dispatches rather than ending...
    await waitFor(() => expect(startCalls()).toHaveLength(2), { timeout: 2000 });
    // ...and nothing partial was recorded as this tab's finished answer.
    expect(sessions.get("search:1")).toBeUndefined();

    act(() =>
      emitResults(bus, lastSearchId(), [
        hit({ path: "src/complete.rs", group: "file" }),
      ]),
    );
    act(() => emitEnded(bus, lastSearchId(), "completed"));

    expect(await screen.findByText("complete.rs")).toBeInTheDocument();
    await waitFor(() => expect(sessions.get("search:1")).toBeDefined());
    expect(sessions.get("search:1")?.reason).toBe("completed");
  });

  it("a cancelled sweep is not recorded as the tab's finished answer", async () => {
    // A project close cancels the sweep (SCC-FR-14). Recording what it had so
    // far as final would leave the tab permanently showing a partial answer.
    const sessions = new SearchSessionStore();
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={sessions}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    act(() =>
      emitResults(bus, lastSearchId(), [hit({ path: "src/a.rs", group: "file" })]),
    );
    await screen.findByText("a.rs");

    act(() => emitEnded(bus, lastSearchId(), "cancelled"));

    await waitFor(() => expect(screen.queryByText(/Searching/i)).toBeNull());
    expect(sessions.get("search:1")).toBeUndefined();
  });

  it("SCH-FR-20: closing the tab cancels its search", async () => {
    const { unmount } = render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));
    const id = lastSearchId();

    unmount();

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "cancel_search" && c[1]?.searchId === id,
        ),
      ).toBe(true),
    );
  });
});

describe("page states (SCH-FR-19 / SCH-FR-21)", () => {
  it("shows an in-progress state until the search ends with nothing", async () => {
    render(
      <SearchResults
        tabId="search:1"
        target={TARGET}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );
    await waitFor(() => expect(searchIds).toHaveLength(1));

    expect(screen.getByText(/Searching/i)).toBeInTheDocument();
    act(() => emitEnded(bus, lastSearchId()));
    expect(await screen.findByText(/No results/i)).toBeInTheDocument();
  });

  it("SCH-FR-21: an invalid regular expression renders in place of the groups", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "start_search") throw new Error("invalid query");
      return undefined;
    });
    render(
      <SearchResults
        tabId="search:1"
        target={{ query: "foo(", mode: "regex" }}
        onActivate={vi.fn()}
        sessions={new SearchSessionStore()}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /Invalid regular expression/i,
    );
  });
});
