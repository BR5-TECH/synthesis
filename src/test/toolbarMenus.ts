/**
 * EDT-FR-UPNB: pick an item of a dropdown of the formatting toolbar's block
 * group, as a pointer does: open the trigger, then press the item. Neither
 * press takes the focus from the editor, so the choice applies to its
 * selection.
 */
import { fireEvent, screen, within } from "@testing-library/react";

/** The trigger of the dropdown named `menu`: "Heading", "Vertical break", "Alignment". */
export function blockMenuTrigger(menu: string): HTMLElement {
  return screen.getByRole("button", { name: menu });
}

/** Open `menu` and press its item named `item`, for example "Heading 2". */
export function chooseToolbarItem(menu: string, item: string): void {
  const trigger = blockMenuTrigger(menu);
  if (trigger.getAttribute("aria-expanded") !== "true") {
    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);
  }
  const popup = screen.getByRole("group", { name: menu });
  const button = within(popup).getByRole("button", { name: item });
  fireEvent.mouseDown(button);
  fireEvent.click(button);
}
