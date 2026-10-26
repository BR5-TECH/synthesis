import { useEffect } from "react";
import { Icon } from "../icons";
import { DIAGRAM_TEMPLATES, type DiagramTemplate } from "./diagramTemplates";
import { useDropdown } from "./useDropdown";

/**
 * EDT-FR-VUGQ / EDT-FR-SCSP / EDT-FR-OWNT: the Insert diagram button of the
 * formatting toolbar and the diagram type menu it opens.
 *
 * The button keeps the selection of the editor when it is pressed, as the
 * other toolbar buttons do, so the fence goes where the caret was. The menu
 * takes focus while it is open; the editor keeps its selection while it does
 * not have focus, and the insert gives focus back to it.
 */
export function DiagramInsertMenu({
  disabled,
  onInsert,
}: {
  /** EDT-FR-VUGQ: true while the surface accepts no input. */
  disabled: boolean;
  onInsert: (template: DiagramTemplate) => void;
}) {
  const menu = useDropdown();
  const { open, setOpen } = menu;
  menu.itemRefs.current.length = DIAGRAM_TEMPLATES.length;

  // A surface that stops accepting input closes the menu it can no longer act on.
  useEffect(() => {
    if (disabled && open) setOpen(false);
  }, [disabled, open, setOpen]);

  const choose = (template: DiagramTemplate) => {
    menu.close(false);
    onInsert(template);
  };

  return (
    <div
      ref={menu.rootRef}
      className="toolbar-dropdown"
      onKeyDown={menu.onRootKeyDown}
    >
      <button
        ref={menu.triggerRef}
        type="button"
        className="btn btn--ghost btn--sm"
        title="Insert diagram"
        aria-label="Insert diagram"
        aria-haspopup="menu"
        aria-expanded={open}
        data-active={open}
        disabled={disabled}
        onMouseDown={(e) => e.preventDefault()}
        onClick={menu.toggle}
        onKeyDown={menu.onTriggerKeyDown}
      >
        <Icon.Diagram size={14} />
      </button>
      {open && (
        <div
          role="menu"
          aria-label="Diagram type"
          className="menu toolbar-dropdown__panel"
          data-testid="diagram-type-menu"
        >
          {DIAGRAM_TEMPLATES.map((template, index) => (
            <button
              key={template.key}
              ref={(el) => {
                menu.itemRefs.current[index] = el;
              }}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className="menu-item toolbar-dropdown__item"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => choose(template)}
              onKeyDown={(e) => menu.onItemKeyDown(e, index)}
            >
              {template.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
