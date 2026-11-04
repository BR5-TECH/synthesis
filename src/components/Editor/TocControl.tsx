import { useState, useSyncExternalStore } from "react";
import { FilterableSelect, type FilterableOption } from "../FilterableSelect";
import { Icon } from "../icons";
import type { OutlineEntry } from "./outline";
import type { DocumentOutline } from "./useDocumentOutline";

const NONE: readonly OutlineEntry[] = [];
const noSubscription = () => () => {};
const noEntries = () => NONE;

/**
 * EDT-FR-IZHD: the filter keeps the entries whose heading text contains it,
 * with no regard to case. The id of an entry is its index, so it never
 * matches.
 */
export function matchesHeading(option: FilterableOption, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  return (option.title ?? "").toLowerCase().includes(needle);
}

/**
 * EDT-FR-LGQA / EDT-FR-PNGU / EDT-FR-QWZN / EDT-FR-IZHD / EDT-FR-EBNU /
 * EDT-FR-AQCR /
 * EDT-FR-KOUB: the table of contents control and the panel it opens, in the
 * action cluster of an Editor tab and of the live prompt of a New Artifact tab
 * (NAW-FR-TFFX). The panel is the filterable selector of
 * `AII-ai-integrations.md` AII-FR-36 and AII-FR-38.
 *
 * The panel reads the outline only while it is open, so a closed panel costs
 * nothing on a long document, and it follows each edit while it stays open
 * (EDT-FR-KJLQ).
 */
export function TocControl({ outline }: { outline: DocumentOutline }) {
  const [open, setOpen] = useState(false);
  const entries = useSyncExternalStore(
    open ? outline.subscribe : noSubscription,
    open ? outline.getSnapshot : noEntries,
  );
  // EDT-FR-QWZN: a heading with no text is not listed. The id stays the
  // index into `entries`, so a dropped heading moves no other entry.
  const options: FilterableOption[] = entries.flatMap((entry, index) =>
    entry.text.trim() === ""
      ? []
      : [
          {
            id: String(index),
            label: entry.text,
            title: entry.text,
            level: entry.level,
            ariaLabel: `${entry.text}, heading level ${entry.level}`,
          },
        ],
  );
  // EDT-FR-EBNU: a document whose headings all have no text has no heading to
  // list.
  const listed = options.length;

  return (
    <FilterableSelect
      options={options}
      value=""
      onChange={(id) => {
        const entry = entries[Number(id)];
        if (entry) outline.reveal(entry);
      }}
      label="Table of contents"
      testId="toc"
      matches={matchesHeading}
      emptyText={listed === 0 ? "No headings" : "No heading matches"}
      fitWindow
      alwaysNotify
      returnFocus={false}
      menuWidth={320}
      menuClassName={listed === 0 ? "toc__panel toc__panel--empty" : "toc__panel"}
      className="toc"
      onOpenChange={setOpen}
      trigger={{ content: <Icon.Toc size={15} />, className: "btn btn--ghost btn--icon toc__trigger" }}
    />
  );
}
