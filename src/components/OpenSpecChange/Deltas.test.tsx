import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import { DeltaList, groupDeltas } from "./Deltas";
import { CHANGED_EXPIRY, LIVE_EXPIRY, demoChange } from "./fixtures";
import type { ChangeDelta } from "../../types";

afterEach(cleanup);

const deltas = demoChange().deltas;
const files = demoChange().artifacts[1].files;

function drawAll() {
  render(<DeltaList deltas={deltas} files={files} />);
}

/** The delta articles of one capability, in order. */
const deltasOf = (capability: string) =>
  within(screen.getByRole("region", { name: `Capability ${capability}` })).getAllByTestId(
    "openspec-delta",
  );

const one = (delta: ChangeDelta) => {
  render(<DeltaList deltas={[delta]} files={[]} />);
  return screen.getByTestId("openspec-delta");
};

describe("the deltas of the specs artifact", () => {
  it("OCW-FR-OUXA: groups the deltas by capability in the order the read gives them, each headed by its id and whether it exists", () => {
    drawAll();
    const groups = screen.getAllByTestId("openspec-capability");
    expect(groups.map((g) => within(g).getByRole("heading", { level: 3 }).textContent)).toEqual([
      "auth",
      "ui",
    ]);
    expect(groups[0]).toHaveTextContent("The live specs hold this capability");
    expect(groups[1]).toHaveTextContent("The live specs do not hold this capability");
    expect(deltasOf("auth").map((d) => d.getAttribute("data-op"))).toEqual([
      "MODIFIED",
      "ADDED",
      "REMOVED",
      "RENAMED",
    ]);
    expect(deltasOf("ui").map((d) => d.getAttribute("data-op"))).toEqual(["MODIFIED", "REMOVED"]);
    expect(groupDeltas(deltas).map((g) => g.deltas.length)).toEqual([4, 2]);
  });

  it("OCW-FR-OUXA: keeps the order of the read for the groups and for the deltas in a group, and sorts nothing", () => {
    // The read gives ui first, and interleaves the two capabilities.
    const read = [deltas[4], deltas[0], deltas[3], deltas[5], deltas[1]];
    render(<DeltaList deltas={read} files={[]} />);
    const groups = screen.getAllByTestId("openspec-capability");
    expect(groups.map((g) => within(g).getByRole("heading", { level: 3 }).textContent)).toEqual([
      "ui",
      "auth",
    ]);
    expect(deltasOf("ui").map((d) => d.getAttribute("data-op"))).toEqual(["REMOVED", "MODIFIED"]);
    expect(deltasOf("auth").map((d) => d.getAttribute("data-op"))).toEqual([
      "MODIFIED",
      "RENAMED",
      "ADDED",
    ]);
    expect(groupDeltas(read).map((g) => g.capability)).toEqual(["ui", "auth"]);
  });

  it("OCW-FR-NEQF: an ADDED delta renders the name, the text, and the scenarios, marked as added", () => {
    const article = one(deltas[1]);
    expect(within(article).getByText("ADDED")).toBeInTheDocument();
    expect(within(article).getByText("Logout")).toBeInTheDocument();
    const added = within(article).getByLabelText("Added requirement");
    expect(added).toHaveTextContent("Users SHALL log out from every page.");
    expect(added).toHaveTextContent("Scenario: Menu");
    expect(added).toHaveTextContent("the session ends");
  });

  it("OCW-FR-WIKH: a MODIFIED delta renders the live and the changed requirement side by side, live first, both whole", () => {
    const article = one(deltas[0]);
    const sides = article.querySelectorAll(".openspec-delta__sides > section");
    expect(sides).toHaveLength(2);
    expect(sides[0]).toHaveAccessibleName("Live requirement");
    expect(sides[1]).toHaveAccessibleName("Changed requirement");
    expect(sides[0]).toHaveTextContent(LIVE_EXPIRY.text);
    expect(sides[0]).toHaveTextContent("Scenario: Idle");
    expect(sides[0]).toHaveTextContent("idle for 60 minutes");
    expect(sides[1]).toHaveTextContent(CHANGED_EXPIRY.text);
    expect(sides[1]).toHaveTextContent("idle for 30 minutes");
  });

  it("OCW-FR-YHIX: a MODIFIED delta with no live requirement renders the changed one alone, and says the live specs hold none of that name", () => {
    const article = one(deltas[3]);
    expect(article.querySelector(".openspec-delta__sides")).toBeNull();
    expect(within(article).getByLabelText("Changed requirement")).toHaveTextContent(
      "The header SHALL show a Log out button.",
    );
    expect(within(article).getByTestId("openspec-no-live")).toHaveTextContent(
      "The live specs hold no requirement named “Header”.",
    );
  });

  it("OCW-FR-FPFE: a REMOVED delta renders the live requirement struck through", () => {
    const article = one(deltas[2]);
    const removed = within(article).getByLabelText("Removed requirement");
    expect(removed.tagName).toBe("DEL");
    expect(removed).toHaveTextContent("Sessions SHALL survive a restart.");
    expect(article.querySelector("s")).toHaveTextContent("Remember me");
  });

  it("OCW-FR-FPFE: a REMOVED delta with no live requirement renders its name alone, with a statement", () => {
    const article = one(deltas[4]);
    expect(article.querySelector("del")).toBeNull();
    expect(article).toHaveTextContent("Footer link");
    expect(within(article).getByTestId("openspec-no-live")).toHaveTextContent(
      "The live specs hold no requirement named “Footer link”.",
    );
  });

  it("OCW-FR-IGTS: a RENAMED delta renders the old and the new name, and the live requirement under its old name", () => {
    const article = one(deltas[5]);
    expect(article.querySelector(".openspec-delta__from")).toHaveTextContent("Login");
    expect(article.querySelector(".openspec-delta__to")).toHaveTextContent("Sign in");
    const live = within(article).getByLabelText("Live requirement");
    expect(live).toHaveTextContent("Live, as “Login”");
    expect(live).toHaveTextContent("Users SHALL log in with a password.");
  });

  it("OCW-FR-IGTS: a RENAMED delta with no live requirement renders the two names alone", () => {
    const article = one({ ...deltas[5], live: null });
    expect(article).toHaveTextContent("Login → Sign in");
    expect(within(article).queryByLabelText("Live requirement")).toBeNull();
  });

  it("OCW-FR-NOTT: a requirement text renders no raw HTML as an element", () => {
    const article = one({
      ...deltas[1],
      requirement: { name: "X", text: "<script>alert(1)</script> and <i>tags</i>", scenarios: [] },
    });
    expect(article.querySelector("script, i")).toBeNull();
    expect(article).toHaveTextContent("<script>alert(1)</script>");
  });
});
