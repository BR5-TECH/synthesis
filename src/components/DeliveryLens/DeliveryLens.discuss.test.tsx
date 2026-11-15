/**
 * The **Discuss** entry of the items and milestones of the Delivery lens
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-HULA,
 * MDL-FR-QELI, MDL-FR-JBEL; `LCM-library-context-menu.md` LCM-FR-07).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { resetListeners } from "../../test/flowMocks";
import { itemEl, selectItem } from "../../test/flowFixtures";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  deliveryBackend,
  fileSource,
  githubAnswer,
  githubSource,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";

const FLOW = "plans/release.flow";
const DOC = plan([
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
]);

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend(DOC, githubAnswer(DOC));
  b.plan = () => githubAnswer(DOC);
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openGithub() {
  const r = await renderLens({ source: async () => githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

async function openFile() {
  const r = await renderLens({ source: async () => fileSource(FLOW), host: { claims: claimsOf() } });
  await screen.findByTestId("flow-item-t1");
  return r;
}

const menu = () => screen.queryByRole("menu", { name: "Node actions" });

describe("the Discuss entry of a GitHub plan (MDL-FR-HULA, MDL-FR-QELI)", () => {
  it("MDL-FR-HULA: a right-click on an item opens a menu that holds a Discuss entry with a leading icon", async () => {
    await openGithub();
    expect(menu()).toBeNull();
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    const entry = within(screen.getByRole("menu", { name: "Node actions" })).getByRole("menuitem", { name: "Discuss" });
    // LCM-FR-07: the entry carries a leading icon.
    expect(entry.querySelector("svg")).not.toBeNull();
    expect(entry.firstElementChild?.tagName.toLowerCase()).toBe("svg");
  });

  it("MDL-FR-HULA: a right-click on a milestone opens the same menu", async () => {
    await openGithub();
    fireEvent.contextMenu(itemEl("m1"), { clientX: 30, clientY: 40 });
    expect(screen.getByRole("menuitem", { name: "Discuss" })).toBeInTheDocument();
  });

  it("MDL-FR-HULA: the context-menu key and Shift+F10 on a focused item open the menu", async () => {
    await openGithub();
    fireEvent.keyDown(itemEl("t1"), { key: "ContextMenu" });
    expect(menu()).not.toBeNull();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(menu()).toBeNull();
    fireEvent.keyDown(itemEl("m1"), { key: "F10", shiftKey: true });
    expect(menu()).not.toBeNull();
  });

  it("MDL-FR-QELI: Discuss hands the node reference of the item to the column, closes the menu, and does not create a thread", async () => {
    const { store } = await openGithub();
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(menu()).toBeNull();
    const { discussion } = store.snapshot();
    expect(discussion.visible).toBe(true);
    expect(discussion.node).toEqual({ lens: "delivery", kind: "item", id: "t1", label: "T1" });
    expect(b.calls.filter((c) => c.cmd.startsWith("get_or_create_map")).length).toBe(0);
  });

  it("MDL-FR-QELI: Discuss on a milestone hands a node of the kind milestone", async () => {
    const { store } = await openGithub();
    fireEvent.contextMenu(itemEl("m1"), { clientX: 30, clientY: 40 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(store.snapshot().discussion.node).toEqual({ lens: "delivery", kind: "milestone", id: "m1", label: "M1" });
  });

  it("MDL-FR-QELI: Discuss raises the signal that moves the focus to the composer of the column", async () => {
    const { store } = await openGithub();
    const before = store.snapshot().discussion.focusSignal;
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(store.snapshot().discussion.focusSignal).toBe(before + 1);
  });
});

describe("the menu keys and the focus (MDL-FR-JBEL)", () => {
  it("MDL-FR-JBEL: the menu opens at the item, takes the arrow keys, and Escape returns the focus to the item", async () => {
    await openGithub();
    const t1 = itemEl("t1");
    t1.focus();
    fireEvent.keyDown(t1, { key: "ContextMenu" });
    const entry = screen.getByRole("menuitem", { name: "Discuss" });
    await waitFor(() => expect(entry).toHaveFocus());
    fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
    expect(entry).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(menu()).toBeNull();
    expect(t1).toHaveFocus();
  });

  it("MDL-FR-JBEL: Enter on the entry discusses the node", async () => {
    const { store } = await openGithub();
    fireEvent.keyDown(itemEl("t1"), { key: "ContextMenu" });
    const entry = screen.getByRole("menuitem", { name: "Discuss" });
    await waitFor(() => expect(entry).toHaveFocus());
    await userEvent.setup().keyboard("{Enter}");
    expect(store.snapshot().discussion.node).toMatchObject({ id: "t1" });
    expect(menu()).toBeNull();
  });

  it("MDL-FR-JBEL: a right-click does not change the selection of the canvas", async () => {
    const { store } = await openGithub();
    selectItem("f1");
    await waitFor(() => expect(store.snapshot().deliveryNode).toMatchObject({ id: "f1" }));
    fireEvent.pointerDown(itemEl("t1"), { button: 2, pointerId: 2, buttons: 2 });
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    expect(store.snapshot().deliveryNode).toMatchObject({ id: "f1" });
  });

  it("MDL-FR-JBEL: a press outside closes the menu", async () => {
    await openGithub();
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    expect(menu()).not.toBeNull();
    fireEvent.mouseDown(document.body);
    expect(menu()).toBeNull();
  });

  it("MDL-FR-JBEL: where a name is being edited the request opens no menu", async () => {
    await openFile();
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t1"));
    await screen.findByRole("textbox", { name: "Item name" });
    fireEvent.contextMenu(itemEl("f1"), { clientX: 30, clientY: 40 });
    expect(menu()).toBeNull();
  });
});

describe("the Discuss entry of a file plan (MDL-FR-HULA, MDL-FR-QELI)", () => {
  it("MDL-FR-HULA: a right-click on an item of a file plan opens the menu", async () => {
    await openFile();
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    expect(screen.getByRole("menuitem", { name: "Discuss" })).toBeInTheDocument();
  });

  it("MDL-FR-HULA: a milestone of a file plan has the menu too", async () => {
    await openFile();
    fireEvent.contextMenu(itemEl("m1"), { clientX: 30, clientY: 40 });
    expect(screen.getByRole("menuitem", { name: "Discuss" })).toBeInTheDocument();
  });

  it("MDL-FR-QELI: Discuss on an item of a file plan hands its reference to the column", async () => {
    const { store } = await openFile();
    fireEvent.contextMenu(itemEl("t1"), { clientX: 30, clientY: 40 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(store.snapshot().discussion.node).toEqual({ lens: "delivery", kind: "item", id: "t1", label: "T1" });
  });
});
