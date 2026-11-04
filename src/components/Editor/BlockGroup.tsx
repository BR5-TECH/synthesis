import { useState } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { Icon } from "../icons";
import {
  caretAlignment,
  selectedBreakLevel,
  setBlockAlignment,
  setVerticalBreak,
} from "../markdownMarkup";
import { BREAK_LEVELS, breakLabel, type MarkerAlignment } from "../../markdown/markers";
import { logDebug } from "../../logging";
import { ToolbarDropdown } from "./ToolbarDropdown";

type MenuKey = "heading" | "break" | "align";

const HEADING_LEVELS = [1, 2, 3, 4] as const;

const ALIGN_ITEMS: Array<{
  key: "left" | MarkerAlignment;
  value: MarkerAlignment | null;
  label: string;
  icon: (p: { size?: number }) => React.ReactNode;
}> = [
  { key: "left", value: null, label: "Align left", icon: Icon.AlignLeft },
  { key: "center", value: "center", label: "Align center", icon: Icon.AlignCenter },
  { key: "right", value: "right", label: "Align right", icon: Icon.AlignRight },
];

/**
 * EDT-FR-UPNB: the block group of the formatting toolbar, the Hx, Vx, and Ax
 * dropdowns in that order. The group holds which of them is open, so at most
 * one is open at a time (EDT-FR-IRNA).
 *
 * `edit` runs a markup edit as one step of the history (EDT-FR-RZTS).
 */
export function BlockGroup({
  editor,
  editable,
  edit,
}: {
  editor: TiptapEditor | null;
  editable: boolean;
  edit: (run: () => boolean) => boolean;
}) {
  const [open, setOpen] = useState<MenuKey | null>(null);
  const opener = (key: MenuKey) => (next: boolean) =>
    setOpen((current) => (next ? key : current === key ? null : current));

  const state = editor && !editor.isDestroyed ? editor.state : null;
  const heading = HEADING_LEVELS.find((level) => editor?.isActive("heading", { level })) ?? null;
  const breakLevel = state ? selectedBreakLevel(state) : null;
  const align = state ? caretAlignment(state) : null;
  const AlignIcon = align === "center" ? Icon.AlignCenter : align === "right" ? Icon.AlignRight : null;

  return (
    <>
      {/* EDT-FR-NTHC: H1 to H4. The command makes a heading of the level a
          paragraph again. */}
      <ToolbarDropdown
        label="Heading"
        testId="heading-menu"
        trigger={heading ? `H${heading}` : "Hx"}
        choice={heading ? `h${heading}` : undefined}
        disabled={!editable}
        open={open === "heading"}
        onOpenChange={opener("heading")}
        items={HEADING_LEVELS.map((level) => ({
          key: `h${level}`,
          label: `Heading ${level}`,
          content: `H${level}`,
          pressed: heading === level,
          onSelect: () => {
            editor?.chain().focus().toggleHeading({ level }).run();
          },
        }))}
      />
      {/* EDT-FR-HDCZ / EDT-FR-UXCK: V1 to V4. */}
      <ToolbarDropdown
        label="Vertical break"
        testId="break-menu"
        trigger={breakLevel ? `V${breakLevel}` : "Vx"}
        choice={breakLevel ? `v${breakLevel}` : undefined}
        disabled={!editable}
        open={open === "break"}
        onOpenChange={opener("break")}
        items={BREAK_LEVELS.map((level) => ({
          key: `v${level}`,
          label: breakLabel(level),
          content: `V${level}`,
          pressed: breakLevel === level,
          onSelect: () => {
            if (!editor) return;
            const changed = edit(() => setVerticalBreak(editor, level));
            logDebug(["frontend"], "vertical break chosen", {
              level,
              resized: breakLevel !== null,
              changed,
            });
          },
        }))}
      />
      {/* EDT-FR-EUUI / EDT-FR-MAVJ: Left, Center, and Right. */}
      <ToolbarDropdown
        label="Alignment"
        testId="align-menu"
        trigger={AlignIcon ? <AlignIcon size={14} /> : "Ax"}
        choice={align ?? undefined}
        disabled={!editable}
        open={open === "align"}
        onOpenChange={opener("align")}
        items={ALIGN_ITEMS.map((item) => ({
          key: item.key,
          label: item.label,
          content: <item.icon size={14} />,
          pressed: (align ?? null) === item.value,
          onSelect: () => {
            if (!editor) return;
            const changed = edit(() => setBlockAlignment(editor, item.value));
            logDebug(["frontend"], "block alignment chosen", { align: item.key, changed });
          },
        }))}
      />
    </>
  );
}
