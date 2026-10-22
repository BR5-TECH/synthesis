/**
 * A scroll offset and a content resize for jsdom, which lays nothing out.
 *
 * `installFakeScroll` replaces `scrollTop` on every element with a stored
 * value. An element scrolls as far as its `data-max-scroll` attribute or a
 * `limitScroll` selector allows (no limit when it has neither), which is how a
 * test says that the content is not yet tall enough.
 * `installFakeResizeObserver` replaces `ResizeObserver` with one that fires
 * only when the test calls `resizeAll`.
 *
 * Both return the function that puts the original back.
 */

const tops = new WeakMap<Element, number>();

/** Limits by selector, for elements a component creates itself. */
const limits = new Map<string, number>();

/**
 * Let every element that matches `selector` scroll no further than `max`, or
 * remove the limit when `max` is null.
 */
export function limitScroll(selector: string, max: number | null): void {
  if (max === null) limits.delete(selector);
  else limits.set(selector, max);
}

function maxScroll(el: Element): number {
  const max = el.getAttribute("data-max-scroll");
  if (max !== null) return Number(max);
  for (const [selector, limit] of limits) {
    if (el.matches(selector)) return limit;
  }
  return Number.POSITIVE_INFINITY;
}

export function installFakeScroll(): () => void {
  const original = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop");
  Object.defineProperty(Element.prototype, "scrollTop", {
    configurable: true,
    get(this: Element) {
      return tops.get(this) ?? 0;
    },
    set(this: Element, value: number) {
      tops.set(this, Math.max(0, Math.min(value, maxScroll(this))));
    },
  });
  return () => {
    limits.clear();
    if (original) Object.defineProperty(Element.prototype, "scrollTop", original);
  };
}

const observers = new Set<FakeResizeObserver>();

class FakeResizeObserver {
  private readonly callback: ResizeObserverCallback;
  readonly targets = new Set<Element>();

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    observers.add(this);
  }

  observe(target: Element): void {
    this.targets.add(target);
  }

  unobserve(target: Element): void {
    this.targets.delete(target);
  }

  disconnect(): void {
    this.targets.clear();
    observers.delete(this);
  }

  fire(): void {
    if (this.targets.size === 0) return;
    this.callback([], this as unknown as ResizeObserver);
  }
}

/** Fire every observer that observes at least one element. */
export function resizeAll(): void {
  [...observers].forEach((o) => o.fire());
}

/** How many observers observe at least one element now. */
export function activeResizeObservers(): number {
  return [...observers].filter((o) => o.targets.size > 0).length;
}

export function installFakeResizeObserver(): () => void {
  const original = globalThis.ResizeObserver;
  globalThis.ResizeObserver =
    FakeResizeObserver as unknown as typeof ResizeObserver;
  return () => {
    observers.clear();
    if (original === undefined) {
      delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
    } else {
      globalThis.ResizeObserver = original;
    }
  };
}
