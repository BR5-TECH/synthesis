/**
 * The links of a Diff tab's Rich rendering (`specifications/ui/DFV-diff-viewer.md`
 * DFV-FR-QJRW): a file link resolves against the compared file, and a fragment
 * jumps to a heading of the target revision.
 */
import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import { useCallback } from "react";
import { isMacPlatform } from "../../platform";
import { followLink, followsOnClick, linkTooltip } from "../linkNavigation";
import { placeOfFile, useLinkHost, useRichLinkHost, type LinkHost } from "../linkFollow";
import { anchorIndex } from "../markdownLinks";

const HEADINGS = ["h1", "h2", "h3", "h4", "h5", "h6"];
/**
 * The headings of the target revision, in document order: those of its static
 * blocks and those of its editable runs.
 */
const TARGET_HEADINGS = [
  ...HEADINGS.map((h) => `.diff-block[data-revision="new"] ${h}`),
  ...HEADINGS.map((h) => `.diff-run ${h}`),
].join(", ");

/**
 * EDT-FR-HMJI / DFV-FR-QJRW: scroll to the heading of the target revision whose
 * anchor is `fragment`, in the rendering that holds `origin`.
 */
export function revealDiffFragment(fragment: string, origin: Element): boolean {
  const root = origin.closest(".diff-rich");
  if (!root) return false;
  const headings = Array.from(root.querySelectorAll<HTMLElement>(TARGET_HEADINGS));
  const heading =
    headings[anchorIndex(headings.map((h) => (h.textContent ?? "").trim()), fragment)];
  if (!heading) return false;
  if (typeof heading.scrollIntoView === "function") {
    heading.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  return true;
}

/** DFV-FR-QJRW: the link host of a Diff tab on the project file `path`. */
export function useDiffLinkHost(path: string): LinkHost {
  return useLinkHost(placeOfFile(path), revealDiffFragment);
}

/**
 * DFV-FR-QJRW / EDT-FR-ZGKO: the destination of an inline link as the static
 * parse gives it, without the angle brackets of `<a b.md>` and without a
 * title such as `a.md "Title"`.
 */
export function staticDestination(raw: string): string {
  const text = raw.trim();
  if (text.startsWith("<")) {
    const close = text.indexOf(">");
    return close > 0 ? text.slice(1, close) : text;
  }
  const space = text.search(/\s/);
  return space < 0 ? text : text.slice(0, space);
}

/**
 * DFV-FR-QJRW: a link of the static rendering. The static rendering takes no
 * input, so a plain click follows the link (EDT-FR-YHKG), and so does Enter
 * on a link that has the focus.
 */
export function StaticLink({ href, children }: { href: string; children: ReactNode }) {
  const host = useRichLinkHost();
  const mac = isMacPlatform();
  const destination = staticDestination(href);
  const follow = useCallback(
    (origin: Element) =>
      followLink(destination, host, (fragment) => host?.revealFragment?.(fragment, origin) ?? false),
    [destination, host],
  );
  const onClick = useCallback(
    (event: MouseEvent<HTMLAnchorElement>) => {
      event.preventDefault();
      if (followsOnClick(event, false, mac)) follow(event.currentTarget);
    },
    [follow, mac],
  );
  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLAnchorElement>) => {
      if (event.key !== "Enter") return;
      event.preventDefault();
      follow(event.currentTarget);
    },
    [follow],
  );
  return (
    <a
      className="diff-link"
      role="link"
      tabIndex={0}
      title={linkTooltip(destination, false, mac)}
      onClick={onClick}
      onKeyDown={onKeyDown}
    >
      {children}
    </a>
  );
}
