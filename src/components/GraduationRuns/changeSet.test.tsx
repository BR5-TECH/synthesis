import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { PathListSection } from "./changeSet";

afterEach(cleanup);

/** The full path of every path row the list renders, in order. */
function leafPaths(): string[] {
  return Array.from(
    screen.getByTestId("list").querySelectorAll<HTMLElement>("[data-path]"),
  ).map((row) => row.dataset.path ?? "");
}

/** Thirty paths in two root folders, each with folders under it. */
const many = Array.from({ length: 30 }, (_, i) =>
  i < 20 ? `src/components/c${i}.tsx` : `src-tauri/src/m${i}.rs`,
).concat("src/state/s.ts", "src-tauri/Cargo.toml");

describe("PathListSection", () => {
  it("GRU-FR-NUCJ: the heading names the list and its count, and opens and closes it", async () => {
    const user = userEvent.setup();
    render(<PathListSection label="Changed" paths={["a.ts", "b.ts"]} testId="list" />);
    const head = screen.getByRole("button", { name: "Changed · 2 paths" });
    // Open when the run is first selected.
    expect(head).toHaveAttribute("aria-expanded", "true");
    expect(leafPaths()).toEqual(["a.ts", "b.ts"]);

    await user.click(head);
    expect(head).toHaveAttribute("aria-expanded", "false");
    expect(leafPaths()).toEqual([]);

    // By keyboard as well.
    head.focus();
    await user.keyboard("{Enter}");
    expect(head).toHaveAttribute("aria-expanded", "true");
    expect(leafPaths()).toEqual(["a.ts", "b.ts"]);
  });

  it("GRU-FR-HEQB: a long list opens its root folders and keeps deeper ones closed", () => {
    render(<PathListSection label="Changed" paths={many} testId="list" />);
    const list = screen.getByTestId("list");
    expect(
      within(list).getByRole("button", { name: "src, 21 paths" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      within(list).getByRole("button", { name: "src-tauri, 11 paths" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      within(list).getByRole("button", { name: "components, 20 paths" }),
    ).toHaveAttribute("aria-expanded", "false");
    // Only the files that stand directly in a root folder are rows.
    expect(leafPaths()).toEqual(["src-tauri/Cargo.toml"]);
  });

  it("GRU-FR-HEQB: a folder row opens and closes its folder by pointer and by keyboard", async () => {
    const user = userEvent.setup();
    render(<PathListSection label="Changed" paths={many} testId="list" />);
    const folder = screen.getByRole("button", { name: "components, 20 paths" });

    await user.click(folder);
    expect(folder).toHaveAttribute("aria-expanded", "true");
    expect(leafPaths()).toContain("src/components/c0.tsx");

    folder.focus();
    await user.keyboard(" ");
    expect(folder).toHaveAttribute("aria-expanded", "false");
    expect(leafPaths()).not.toContain("src/components/c0.tsx");
  });

  it("GRU-FR-HEQB: closing a root folder hides every row under it and keeps the folder's own state", async () => {
    const user = userEvent.setup();
    render(<PathListSection label="Changed" paths={many} testId="list" />);
    await user.click(screen.getByRole("button", { name: "components, 20 paths" }));
    const root = screen.getByRole("button", { name: "src, 21 paths" });

    await user.click(root);
    expect(screen.queryByRole("button", { name: "components, 20 paths" })).toBeNull();

    await user.click(root);
    expect(
      screen.getByRole("button", { name: "components, 20 paths" }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("GRU-FR-HEQB: a list that grows keeps the default rule for the folders nobody toggled", () => {
    const { rerender } = render(
      <PathListSection label="Changed" paths={["src/a/x.ts"]} testId="list" />,
    );
    expect(leafPaths()).toEqual(["src/a/x.ts"]);
    rerender(
      <PathListSection
        label="Changed"
        paths={["src/a/x.ts", "src/b/y.ts"]}
        testId="list"
      />,
    );
    // The new folder `b` is open, as the rule says for a short list.
    expect(leafPaths()).toEqual(["src/a/x.ts", "src/b/y.ts"]);
  });

  it("GRU-FR-TXLW: a path row shows its name and carries its full path", () => {
    render(
      <PathListSection
        label="Changed"
        paths={["src-tauri/src/agent/tests/a.rs", "src-tauri/src/agent/tests/b.rs"]}
        testId="list"
      />,
    );
    // The chain is one row.
    expect(
      screen.getByRole("button", { name: "src-tauri/src/agent/tests, 2 paths" }),
    ).toBeInTheDocument();
    const row = screen.getByTestId("list").querySelector<HTMLElement>(
      '[data-path="src-tauri/src/agent/tests/a.rs"]',
    );
    expect(row).toHaveTextContent(/^a\.rs$/);
    expect(row).toHaveAttribute("title", "src-tauri/src/agent/tests/a.rs");
  });

  it("GRU-FR-WJHV: the hidden list counts and names its paths and says how many are not listed", () => {
    render(
      <PathListSection
        label="Hidden by ignore rules"
        paths={["node_modules/"]}
        omitted={5}
        testId="list"
      />,
    );
    expect(
      screen.getByRole("button", { name: "Hidden by ignore rules · 1 path" }),
    ).toBeInTheDocument();
    expect(leafPaths()).toEqual(["node_modules/"]);
    expect(screen.getByTestId("list")).toHaveTextContent("5 more are not listed.");
  });

  it("GRU-FR-WJHV: the note about unlisted paths stays while the list is closed", async () => {
    const user = userEvent.setup();
    render(
      <PathListSection
        label="Hidden by ignore rules"
        paths={["a/"]}
        omitted={2}
        testId="list"
      />,
    );
    await user.click(screen.getByRole("button", { name: "Hidden by ignore rules · 1 path" }));
    expect(screen.getByTestId("list")).toHaveTextContent("2 more are not listed.");
  });

  it("GRU-FR-MRPE: a path is rendered as text and never as markup", () => {
    const path = "src/<img src=x onerror=alert(1)>.ts";
    render(<PathListSection label="Changed" paths={[path]} testId="list" />);
    const list = screen.getByTestId("list");
    expect(list.querySelector("img")).toBeNull();
    expect(leafPaths()).toEqual([path]);
  });

  it("GRU-FR-TXLW: the rows stand in the bounded box, and each folder shows its count", () => {
    render(<PathListSection label="Changed" paths={many} testId="list" />);
    const list = screen.getByTestId("list");
    const box = list.querySelector(".graduation__paths");
    expect(box).not.toBeNull();
    expect(box!.querySelector("[data-path]")).not.toBeNull();
    expect(box!.querySelector("[data-folder]")).not.toBeNull();
    const src = screen.getByRole("button", { name: "src, 21 paths" });
    expect(src.querySelector(".graduation__path-count")).toHaveTextContent(/^21$/);
  });

  it("GRU-FR-WJHV: a hidden folder shows its own name with its slash", () => {
    render(
      <PathListSection label="Hidden by ignore rules" paths={["node_modules/"]} testId="list" />,
    );
    const row = screen.getByTestId("list").querySelector("[data-path]");
    expect(row).toHaveTextContent(/^node_modules\/$/);
  });

  it("GRU-FR-WJHV: no unlisted paths make no note", () => {
    render(
      <PathListSection label="Hidden by ignore rules" paths={["a/"]} omitted={0} testId="list" />,
    );
    expect(screen.getByTestId("list")).not.toHaveTextContent("not listed");
    expect(screen.getByTestId("list").querySelector("p")).toBeNull();
  });

  it("GRU-FR-TXLW: a path listed twice counts once in the heading and in its folder", () => {
    render(
      <PathListSection label="Changed" paths={["src/a.ts", "src/a.ts"]} testId="list" />,
    );
    expect(screen.getByRole("button", { name: "Changed · 1 path" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "src, 1 path" })).toBeInTheDocument();
  });

  it("GRU-FR-HEQB: 24 paths open every folder, and 25 open only the roots", () => {
    const paths = (count: number) =>
      Array.from({ length: count }, (_, i) => `src/deep/f${i}.ts`).concat("src/top.ts");
    const { rerender } = render(
      <PathListSection label="Changed" paths={paths(23)} testId="list" />,
    );
    expect(screen.getByRole("button", { name: "deep, 23 paths" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    // A list that grows past the limit follows the rule for its new length:
    // the deeper folders nobody toggled close.
    rerender(<PathListSection label="Changed" paths={paths(24)} testId="list" />);
    expect(screen.getByRole("button", { name: "deep, 24 paths" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("GRU-FR-HEQB: a folder the author toggled keeps its state while the list grows", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <PathListSection label="Changed" paths={many} testId="list" />,
    );
    await user.click(screen.getByRole("button", { name: "components, 20 paths" }));
    rerender(
      <PathListSection
        label="Changed"
        paths={[...many, "src/components/new.tsx"]}
        testId="list"
      />,
    );
    expect(
      screen.getByRole("button", { name: "components, 21 paths" }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("GRU-FR-MRPE: a folder name is rendered as text and never as markup", () => {
    render(<PathListSection label="Changed" paths={["<b>x</b>/y.ts"]} testId="list" />);
    const list = screen.getByTestId("list");
    expect(list.querySelector("b")).toBeNull();
    expect(list.querySelector("[data-folder]")).toHaveAttribute("data-folder", "<b>x</b>");
  });
});
