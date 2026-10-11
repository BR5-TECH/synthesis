// A Playwright check can set `window.__nextPick` to choose what the next picker
// returns, and reads `window.__lastOpenOptions` to see how it was opened.
export async function open(options?: unknown) {
  const w = globalThis as Record<string, any>;
  w.__lastOpenOptions = options;
  if ("__nextPick" in w) {
    const pick = w.__nextPick;
    delete w.__nextPick;
    return pick;
  }
  return "/Users/demo/dev/acme";
}
export async function save() {
  return null;
}
export async function message() {}
export async function ask() {
  return true;
}
export async function confirm() {
  return true;
}
