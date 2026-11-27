import { Icon } from "../icons";
import { useSubmenuShift } from "./menuPlacement";

/**
 * LCM-FR-XWFO, LCM-FR-NJVW: how the **Terminal Tab** entry shows. `hidden` on
 * Windows, `disabled` where no worktree is active.
 */
export type TerminalTabEntry = "enabled" | "disabled" | "hidden";

interface OpenInSubmenuProps {
  open: boolean;
  /** LCM-FR-UCJY: hover with intent, and a click that acts at once. */
  onEnter: () => void;
  onLeave: () => void;
  onOpenClick: () => void;
  fileManagerLabel: string;
  terminalTab: TerminalTabEntry;
  onSystemTerminal: () => void;
  onTerminalTab: () => void;
  onRevealInFileManager: () => void;
}

/**
 * LCM-FR-WBHG: the nested **Open In** submenu. It opens on mouse-over, and a
 * click also opens it. Every entry carries a leading icon (LCM-FR-07).
 */
export function OpenInSubmenu({
  open,
  onEnter,
  onLeave,
  onOpenClick,
  fileManagerLabel,
  terminalTab,
  onSystemTerminal,
  onTerminalTab,
  onRevealInFileManager,
}: OpenInSubmenuProps) {
  const tabEnabled = terminalTab === "enabled";
  // LCM-FR-PDXH: the submenu moves up when it would overhang the window bottom.
  const shift = useSubmenuShift(open);
  return (
    <div className="menu-item--parent" onMouseEnter={onEnter} onMouseLeave={onLeave}>
      <div className="menu-item" aria-expanded={open} onClick={onOpenClick}>
        <Icon.ExternalLink size={13} /> Open In
        <span className="menu-item__kbd">
          <Icon.CaretRight size={12} />
        </span>
      </div>
      {open && (
        <div className="menu menu--sub" ref={shift.ref} style={shift.style}>
          {/* LCM-FR-PAUK */}
          <div className="menu-item" onClick={onSystemTerminal}>
            <Icon.TerminalPrompt size={13} /> System Terminal
          </div>
          {/* LCM-FR-XWFO, LCM-FR-NJVW */}
          {terminalTab !== "hidden" && (
            <div
              className={tabEnabled ? "menu-item" : "menu-item menu-item--disabled"}
              aria-disabled={!tabEnabled}
              onClick={() => {
                if (tabEnabled) onTerminalTab();
              }}
            >
              <Icon.Terminal size={13} /> Terminal Tab
            </div>
          )}
          {/* LCM-FR-VTLR */}
          <div className="menu-item" onClick={onRevealInFileManager}>
            <Icon.FolderOpen size={13} /> {fileManagerLabel}
          </div>
        </div>
      )}
    </div>
  );
}
