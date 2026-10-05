/**
 * CVP-FR-40 / CMT-FR-70: the keyboard rule every composer posts with.
 *
 * **Ctrl+Enter** on Windows and Linux and **Cmd+Enter** on macOS post the
 * composer. Both modifiers are read on every platform, because a keyboard
 * attached to another platform's machine still has the key it has.
 *
 * Pure so the rule is testable without a DOM. The mention picker is not part of
 * it: while the picker is open the composer's text area never offers this key
 * to its owner, so Enter completes a nickname where the picker is open and this
 * rule posts where it is not (AGT-FR-27).
 */

/** The parts of a keyboard event the rule reads. */
export interface PostKeyEvent {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  keyCode?: number;
  nativeEvent?: { isComposing?: boolean; keyCode?: number };
}

/** The key code browsers report for any key pressed during an IME composition. */
const IME_KEY_CODE = 229;

/**
 * Whether an IME composition is in progress.
 *
 * `isComposing` is what a commit keypress carries. Some engines report the end
 * of a composition with `keyCode` 229 and no flag, so both are read. Posting on
 * either would send a half-typed word and eat the commit.
 */
export function isImeComposing(event: PostKeyEvent): boolean {
  return (
    event.nativeEvent?.isComposing === true ||
    event.keyCode === IME_KEY_CODE ||
    event.nativeEvent?.keyCode === IME_KEY_CODE
  );
}

/** Whether this key press is the post accelerator, before any state is checked. */
export function isPostAccelerator(event: PostKeyEvent): boolean {
  if (event.key !== "Enter") return false;
  if (!(event.metaKey || event.ctrlKey)) return false;
  return !isImeComposing(event);
}

/** The hint the composer shows for the accelerator, in the platform's own words. */
export function postAcceleratorHint(
  platform: string = typeof navigator === "undefined" ? "" : navigator.platform,
): string {
  return /mac|iphone|ipad/i.test(platform) ? "⌘↵" : "Ctrl↵";
}
