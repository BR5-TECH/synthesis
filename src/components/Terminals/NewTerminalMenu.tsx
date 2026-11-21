/**
 * The New terminal menu (`TRM-terminals.md` TRM-FR-JINY … TRM-FR-IVGX).
 *
 * A dropdown under the button that opened it. It reads the targets each time it
 * opens. The kinds of the active worktree are top-level entries, so the usual
 * terminal is one click away. Every other worktree and work stream is a
 * submenu entry that holds the same kind list for its target. A kind that is
 * not available is greyed out with its reason in plain text.
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import { errorCode, targetKey, upsertTerminal } from "../../state/terminals";
import { kindName, reasonText, roleName, type TargetRole } from "../../state/terminalGroups";
import type { KindAvailability, TerminalError, TerminalSummary, TerminalTarget } from "../../types";
import { Icon } from "../icons";

const KIND_ORDER = ["host_shell", "container_shell", "microsandbox_shell", "agent_cli"] as const;
const BUSY_LINE = "A graduation run is working in this target.";
const EDGE = 8;
const GAP = 4;

function roleOf(target: TerminalTarget): TargetRole {
  if (target.target.kind === "work_stream") return "stream";
  return target.isActiveWorktree ? "active" : "worktree";
}

/** TRM-FR-AASW: active worktree, other worktrees by name, work streams by name. */
export function orderTargets(targets: readonly TerminalTarget[]): TerminalTarget[] {
  const byName = (a: TerminalTarget, b: TerminalTarget) => a.name.localeCompare(b.name);
  return [
    ...targets.filter((t) => roleOf(t) === "active"),
    ...targets.filter((t) => roleOf(t) === "worktree").sort(byName),
    ...targets.filter((t) => roleOf(t) === "stream").sort(byName),
  ];
}

/** TRM-FR-CZTR: the kinds in their fixed order, Agent CLI entries last. */
export function orderKinds(kinds: readonly KindAvailability[]): KindAvailability[] {
  return [...kinds].sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind));
}

function kindKey(kind: KindAvailability): string {
  return kind.vendor ? `${kind.kind}:${kind.vendor}` : kind.kind;
}

function placeLabel(kind: KindAvailability): string {
  return kind.runsIn === "devcontainer" ? "dev container" : kind.runsIn === "microsandbox" ? "vm" : kind.runsIn;
}

function entryName(kind: KindAvailability): string {
  return kind.kind === "agent_cli" && kind.vendor
    ? kindName(kind.kind, kind.vendor).replace("Agent CLI · ", "")
    : kindName(kind.kind);
}

function kindIcon(kind: KindAvailability): ReactNode {
  if (kind.kind === "host_shell") return <Icon.Terminal size={13} aria-hidden="true" />;
  if (kind.kind === "container_shell") return <Icon.Boxes size={13} aria-hidden="true" />;
  if (kind.kind === "microsandbox_shell") return <Icon.Monitor size={13} aria-hidden="true" />;
  return <Icon.Agents size={13} aria-hidden="true" />;
}

function failureText(err: unknown): string {
  const code = errorCode(err);
  const typed = err as Partial<TerminalError> | null;
  if (code === "terminal_limit_reached") return "The project holds the most terminals it can hold (16).";
  if (code === "target_gone") return "This target is gone.";
  if (code === "kind_unavailable") return reasonText(typed?.reason);
  if (code === "platform_unsupported") return "Terminals are not available on this platform.";
  return typed?.message ? `The terminal did not start: ${typed.message}` : "The terminal did not start.";
}

const ITEM = '[role="menuitem"]';

/** The menu items that belong to `menu` itself, not to a submenu inside it. */
function ownItems(menu: Element): HTMLElement[] {
  return Array.from(menu.querySelectorAll<HTMLElement>(ITEM)).filter((el) => el.closest('[role="menu"]') === menu);
}

interface Failure {
  target: string;
  text: string;
}

/** The kind entries of one target, with its busy line and its create error. */
function KindEntries({
  target,
  failure,
  onChoose,
}: {
  target: TerminalTarget;
  failure: Failure | null;
  onChoose: (target: TerminalTarget, kind: KindAvailability) => void;
}) {
  const key = targetKey(target.target);
  return (
    <>
      {/* TRM-FR-IVGX: the busy line disables no entry. */}
      {target.busyRunId && (
        <p className="new-terminal-menu__busy" role="note">
          <Icon.AlertTriangle size={11} aria-hidden="true" />
          {BUSY_LINE}
        </p>
      )}
      {orderKinds(target.kinds).map((kind) => {
        const detail = kind.available ? placeLabel(kind) : reasonText(kind.reason);
        return (
          <div
            key={kindKey(kind)}
            role="menuitem"
            tabIndex={-1}
            className={kind.available ? "menu-item new-terminal-menu__kind" : "menu-item menu-item--disabled new-terminal-menu__kind"}
            aria-label={entryName(kind)}
            aria-description={detail}
            aria-disabled={kind.available ? undefined : true}
            data-available={kind.available ? "true" : undefined}
            data-unavailable={kind.available ? undefined : "true"}
            onClick={() => {
              // TRM-FR-HJUD: a greyed entry does nothing when chosen.
              if (kind.available) onChoose(target, kind);
            }}
          >
            {kindIcon(kind)}
            <span className="new-terminal-menu__text">
              <span className="new-terminal-menu__name">{entryName(kind)}</span>
              {!kind.available && <span className="new-terminal-menu__reason">{detail}</span>}
            </span>
            {kind.available && <span className="new-terminal-menu__place">{detail}</span>}
          </div>
        );
      })}
      {/* TRM-FR-YVPJ: the error shows under the kind entries of its target. */}
      {failure?.target === key && (
        <p className="new-terminal-menu__error" role="alert">
          {failure.text}
        </p>
      )}
    </>
  );
}

export function NewTerminalMenu({
  anchor,
  onClose,
  onCreated,
}: {
  /** The button that opened the menu. */
  anchor: HTMLElement;
  /** `restoreFocus` is true for Escape, which puts focus back on `anchor`. */
  onClose: (restoreFocus: boolean) => void;
  onCreated: (session: TerminalSummary) => void;
}) {
  const [targets, setTargets] = useState<TerminalTarget[] | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const [openSub, setOpenSub] = useState<string | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const busy = useRef(false);
  const mounted = useRef(true);
  const focusSub = useRef<string | null>(null);
  const frameRef = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const loaded = targets !== null;
  const placed = pos !== null;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    api
      .listTerminalTargets()
      .then((list) => {
        if (live) setTargets(orderTargets(list));
      })
      .catch((err) => {
        // TRM-FR-QJSW
        logWarn(["frontend"], "terminal targets read failed", { code: errorCode(err) });
        if (live) setLoadFailed(true);
      });
    return () => {
      live = false;
    };
  }, []);

  const active = useMemo(() => targets?.find((t) => roleOf(t) === "active") ?? null, [targets]);
  const others = useMemo(() => targets?.filter((t) => roleOf(t) !== "active") ?? [], [targets]);

  // The menu aligns its trailing edge with the button, stays inside the window,
  // and opens above the button where the space below is too small.
  useLayoutEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const box = anchor.getBoundingClientRect();
    const width = frame.offsetWidth;
    const height = frame.offsetHeight;
    const left = Math.max(EDGE, Math.min(box.right - width, window.innerWidth - width - EDGE));
    const below = box.bottom + GAP;
    const above = box.top - GAP - height;
    const bottomLimit = window.innerHeight - height - EDGE;
    // Below where it fits, else above where it fits, else as low as the window lets it.
    const top = below <= bottomLimit ? below : above >= EDGE ? above : Math.min(below, bottomLimit);
    setPos({ left, top: Math.max(EDGE, top) });
  }, [anchor, loaded, loadFailed, failure]);

  // TRM-FR-PVTT: the focus goes to the first available kind of the active
  // worktree, else to the first entry. Until then the frame holds it. A
  // hidden element takes no focus, so this waits until the menu is placed.
  useLayoutEffect(() => {
    const frame = frameRef.current;
    if (!frame || !placed) return;
    if (!loaded) {
      frame.focus();
      return;
    }
    const items = ownItems(frame);
    (items.find((el) => el.dataset.available === "true" && !el.dataset.submenu) ?? items[0] ?? frame).focus();
  }, [loaded, placed]);

  // TRM-FR-JZPT: an activation outside the menu and its submenu closes both.
  // The opening button toggles the menu itself, so it does not count.
  useEffect(() => {
    const outside = (target: EventTarget | null) => {
      const node = target as Node | null;
      return !!node && !frameRef.current?.contains(node) && !anchor.contains(node);
    };
    const onPointer = (e: Event) => {
      if (outside(e.target)) closeRef.current(false);
    };
    const onFocus = (e: FocusEvent) => {
      if (outside(e.target)) closeRef.current(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      // TRM-FR-VJXC: Escape in a submenu closes the submenu alone.
      const sub = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-submenu-of]");
      if (sub && frameRef.current?.contains(sub)) {
        closeSubmenu(sub.dataset.submenuOf!);
        return;
      }
      closeRef.current(true);
    };
    document.addEventListener("mousedown", onPointer, true);
    document.addEventListener("contextmenu", onPointer, true);
    document.addEventListener("focusin", onFocus, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onPointer, true);
      document.removeEventListener("contextmenu", onPointer, true);
      document.removeEventListener("focusin", onFocus, true);
      window.removeEventListener("keydown", onKey, true);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anchor]);

  const parentEntry = (key: string) =>
    Array.from(frameRef.current?.querySelectorAll<HTMLElement>("[data-submenu]") ?? []).find(
      (el) => el.dataset.submenu === key,
    ) ?? null;

  const focusFirstOf = (key: string) => {
    const sub = Array.from(frameRef.current?.querySelectorAll<HTMLElement>("[data-submenu-of]") ?? []).find(
      (el) => el.dataset.submenuOf === key,
    );
    if (sub) ownItems(sub)[0]?.focus();
  };

  // TRM-FR-VJXC: a submenu opened by a key takes the focus on its first entry.
  useLayoutEffect(() => {
    if (!openSub || focusSub.current !== openSub) return;
    focusSub.current = null;
    focusFirstOf(openSub);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openSub]);

  function closeSubmenu(key: string) {
    setOpenSub(null);
    parentEntry(key)?.focus();
  }

  /**
   * TRM-FR-VJXC: a pointer opens one submenu at a time. When that closes the
   * submenu that holds the focus, the focus goes to its entry, so the keys
   * still reach the menu.
   */
  const hoverSubmenu = (key: string | null) => {
    const held = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>("[data-submenu-of]");
    if (held && held.dataset.submenuOf !== key) parentEntry(held.dataset.submenuOf!)?.focus();
    setOpenSub(key);
  };

  /** TRM-FR-VJXC: one submenu is open at a time; the keys also focus its first entry. */
  const openSubmenu = (key: string, focusFirst: boolean) => {
    if (openSub === key) {
      // A pointer opened it already, so no render follows.
      if (focusFirst) focusFirstOf(key);
      return;
    }
    focusSub.current = focusFirst ? key : null;
    setOpenSub(key);
  };

  /** TRM-FR-FYYB: a chosen kind creates the terminal; other choices wait. */
  const choose = async (target: TerminalTarget, kind: KindAvailability) => {
    if (busy.current) return;
    busy.current = true;
    setFailure(null);
    try {
      const session = await api.createTerminal(target.target, kind.kind, kind.vendor ?? null);
      upsertTerminal(session);
      logDebug(["frontend"], "terminal created", { terminalId: session.id, kind: session.kind });
      // TRM-FR-JZPT: a menu that closed while the call ran opens no tab. The
      // session still shows as a row.
      if (mounted.current) onCreated(session);
    } catch (err) {
      logWarn(["frontend"], "terminal create failed", { code: errorCode(err), kind: kind.kind });
      if (mounted.current) setFailure({ target: targetKey(target.target), text: failureText(err) });
    } finally {
      busy.current = false;
    }
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const focused = document.activeElement as HTMLElement | null;
    const menu = focused?.closest('[role="menu"]');
    if (!menu || !frameRef.current?.contains(menu)) return;
    const items = ownItems(menu);
    const at = items.indexOf(focused as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (items.length === 0) return;
      const step = e.key === "ArrowDown" ? 1 : -1;
      items[(at + step + items.length) % items.length].focus();
      return;
    }
    const submenuKey = focused?.dataset.submenu;
    if ((e.key === "ArrowRight" || e.key === "Enter") && submenuKey) {
      e.preventDefault();
      openSubmenu(submenuKey, true);
      return;
    }
    if (e.key === "ArrowLeft") {
      const of = (menu as HTMLElement).dataset.submenuOf;
      if (of) {
        e.preventDefault();
        closeSubmenu(of);
      }
      return;
    }
    if ((e.key === "Enter" || e.key === " ") && at >= 0) {
      e.preventDefault();
      focused?.click();
    }
  };

  return (
    <div
      ref={frameRef}
      className="menu new-terminal-menu"
      role="menu"
      aria-label="New terminal"
      aria-busy={!loaded && !loadFailed ? true : undefined}
      tabIndex={-1}
      style={{
        position: "fixed",
        left: pos?.left ?? 0,
        top: pos?.top ?? 0,
        visibility: pos ? undefined : "hidden",
        zIndex: "var(--z-overlay)",
      }}
      onKeyDown={onKeyDown}
      // One highlight: the entry under the pointer takes the focus.
      onMouseOver={(e) => {
        const item = (e.target as HTMLElement).closest<HTMLElement>(ITEM);
        if (item && item !== document.activeElement) item.focus();
      }}
      // A scroll moves the entries, so an open submenu no longer sits beside its entry.
      onScroll={() => setOpenSub(null)}
    >
      {loadFailed && (
        <p className="new-terminal-menu__error" role="alert">
          The targets could not be read.
        </p>
      )}
      {!loaded && !loadFailed && (
        <p className="new-terminal-menu__status" role="status">
          Reading targets…
        </p>
      )}
      {active && (
        <div
          role="group"
          aria-label={active.name}
          onMouseEnter={() => hoverSubmenu(null)}
        >
          <div className="new-terminal-menu__head" aria-hidden="true">
            <Icon.Branch size={11} />
            <span className="new-terminal-menu__head-name">{active.name}</span>
            {active.branch && <span className="new-terminal-menu__head-meta">{active.branch}</span>}
            <span className="new-terminal-menu__head-meta">{roleName("active")}</span>
          </div>
          <KindEntries target={active} failure={failure} onChoose={(t, k) => void choose(t, k)} />
        </div>
      )}
      {active && others.length > 0 && <div className="menu-sep" role="separator" />}
      {others.map((target) => {
        const key = targetKey(target.target);
        const expanded = openSub === key;
        const role = roleName(roleOf(target));
        return (
          <div key={key} role="none" className="menu-item--parent">
            <div
              role="menuitem"
              tabIndex={-1}
              aria-haspopup="menu"
              aria-expanded={expanded}
              aria-label={[target.name, target.branch, role, target.busyRunId ? "busy" : null].filter(Boolean).join(", ")}
              className="menu-item new-terminal-menu__target"
              data-submenu={key}
              onMouseEnter={() => hoverSubmenu(key)}
              onClick={() => openSubmenu(key, false)}
            >
              <Icon.Branch size={13} aria-hidden="true" />
              <span className="new-terminal-menu__text">
                <span className="new-terminal-menu__name">{target.name}</span>
                <span className="new-terminal-menu__reason">
                  {target.branch ? `${target.branch} · ${role}` : role}
                </span>
              </span>
              {target.busyRunId && (
                <span className="terminal-busy" title="A graduation run is working in this target">
                  <Icon.AlertTriangle size={11} aria-hidden="true" />
                </span>
              )}
              <span className="menu-item__kbd" aria-hidden="true">
                <Icon.CaretRight size={12} />
              </span>
            </div>
            {expanded && (
              <Submenu label={target.name} submenuOf={key}>
                <KindEntries target={target} failure={failure} onChoose={(t, k) => void choose(t, k)} />
              </Submenu>
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * A submenu flyout (TRM-FR-VJXC). It is fixed to the window, so the scroll of
 * the menu does not clip it. It opens on the trailing side of its entry, or on
 * the leading side where the window has no space, and it stays inside the
 * window.
 */
function Submenu({ label, submenuOf, children }: { label: string; submenuOf: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [place, setPlace] = useState<{ left: number; top: number; flip: boolean } | null>(null);
  // Every render, because a create error changes the height. It sets the
  // state only when the place changes.
  useLayoutEffect(() => {
    const el = ref.current;
    const parent = el?.parentElement;
    if (!el || !parent) return;
    const box = parent.getBoundingClientRect();
    const width = el.offsetWidth;
    const height = el.offsetHeight;
    const flip = box.right + width > window.innerWidth - EDGE && box.left - width >= EDGE;
    const left = flip ? box.left - width : Math.max(EDGE, Math.min(box.right, window.innerWidth - width - EDGE));
    const top = Math.max(EDGE, Math.min(box.top, window.innerHeight - height - EDGE));
    if (!place || place.left !== left || place.top !== top || place.flip !== flip) setPlace({ left, top, flip });
  });
  return (
    <div
      ref={ref}
      role="menu"
      aria-label={label}
      className="menu menu--sub new-terminal-menu__sub"
      data-submenu-of={submenuOf}
      data-flip={place?.flip ? "true" : undefined}
      style={{
        position: "fixed",
        left: place?.left ?? 0,
        top: place?.top ?? 0,
        // Not `visibility: hidden`: the keys move the focus into the submenu
        // before it is placed, and a hidden element takes no focus.
        opacity: place ? undefined : 0,
        pointerEvents: place ? undefined : "none",
      }}
    >
      {children}
    </div>
  );
}
