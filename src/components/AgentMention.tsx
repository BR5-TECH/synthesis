/**
 * The mention picker and composer (`specifications/ui/AGT-agents.md`
 * AGT-FR-25 … AGT-FR-30).
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ALL_HANDLE, activeMention, applyMention } from "./agentTags";
import type { ProjectAgent } from "../types";

import { availabilityShort } from "./Agents";

// ---------------------------------------------------------------------------
// The mention picker (AGT-FR-25 … AGT-FR-27, AGT-FR-30)
// ---------------------------------------------------------------------------

export interface MentionPickerProps {
  /** The project's enrolled agents, already loaded — this issues no call. */
  agents: ProjectAgent[];
  /** What the author has typed after the sigil. */
  query: string;
  /** Which entry the arrows have highlighted, into the *filtered* list. `-1` for none. */
  highlight: number;
  /**
   * Which side of the composer to open on. Above by preference — a picker over
   * the message reads as part of writing it — but the composer's owner flips
   * this when there is not room, because the rail that owns the tallest of them
   * clips what overflows it (`kit.css` `.comment-rail`).
   */
  placement?: "above" | "below";
  /** How tall the picker may grow before it scrolls, given the room it has. */
  maxHeight?: number;
  /** Measured by the composer to decide `placement` and `maxHeight`. */
  boxRef?: React.Ref<HTMLDivElement>;
  /** Called with the chosen entry's tag — a nickname, or the `@all` handle. */
  onChoose: (handle: string) => void;
}

/**
 * The picker's own bound, as a **border-box** height — the picker sets
 * `boxSizing: "border-box"` precisely so this number means the space the picker
 * actually occupies, which is the only thing that can be compared against the
 * room its clipping ancestor leaves it.
 */
const PICKER_MAX_HEIGHT = 180;
const PICKER_GAP = 4;

/**
 * The nearest ancestor that would clip an overflowing child, or null when
 * nothing but the viewport does.
 *
 * The picker is positioned against the composer, but what actually cuts it off
 * is whichever ancestor sets an overflow — in the rail's case a
 * `.comment-rail { overflow: hidden }` several levels up. Asking the DOM rather
 * than hard-coding that selector keeps the composer usable in the New Artifact
 * tab and anywhere else it is lent a margin.
 */
function clippingAncestor(el: HTMLElement | null): HTMLElement | null {
  for (let node = el?.parentElement ?? null; node; node = node.parentElement) {
    const style = getComputedStyle(node);
    if (/auto|scroll|hidden|clip/.test(style.overflow + style.overflowX + style.overflowY)) {
      return node;
    }
  }
  return null;
}

/**
 * AGT-FR-33: the agents a query narrows to — exactly those whose nickname
 * **begins with** it, matched without regard to case — ordered by nickname
 * without regard to case. An empty query offers every enrolled agent.
 *
 * Prefix rather than containment, and sorted here rather than trusted from the
 * caller, because AGT-FR-27 makes this order load-bearing: Tab completes the
 * first selectable entry, so "the top match" has to be a property of this
 * function rather than of whatever order the roster happened to arrive in.
 *
 * Pure and exported so the ranking is testable without rendering.
 */
export function matchingAgents(agents: ProjectAgent[], query: string): ProjectAgent[] {
  const needle = query.trim().toLowerCase();
  const shown = needle
    ? agents.filter((a) => a.agent.nickname.toLowerCase().startsWith(needle))
    : [...agents];
  return shown.sort((a, b) =>
    a.agent.nickname.toLowerCase().localeCompare(b.agent.nickname.toLowerCase()),
  );
}

/**
 * One row of the picker: an enrolled agent, or the `@all` handle that stands for
 * every agent that can answer (AGT-FR-39).
 *
 * A union rather than a `ProjectAgent` with a flag, because the handle genuinely
 * is not an agent — it has no id, no model, and no availability of its own, and
 * what it displays where a model would go is the roster it expands to.
 */
export type MentionEntry =
  | {
      kind: "all";
      /** The agents it currently stands for; empty means it addresses nobody. */
      nicknames: string[];
      selectable: boolean;
    }
  | { kind: "agent"; enrolled: ProjectAgent; selectable: boolean };

/** The tag an entry inserts — a nickname, or the handle itself (AGT-FR-40). */
export function entryHandle(entry: MentionEntry): string {
  return entry.kind === "all" ? ALL_HANDLE : entry.enrolled.agent.nickname;
}

/**
 * AGT-FR-39 / AGT-FR-33: the picker's rows for a query, in the order they render.
 *
 * `@all` **leads every nickname** whenever the typed fragment is a prefix of
 * `all`, however the nicknames sort — an empty fragment included, since it is a
 * prefix of everything. That position is load-bearing rather than cosmetic:
 * AGT-FR-33 makes the top match the first selectable row, and AGT-FR-27 has Tab
 * complete it, so in a project enrolling `@arch` a bare `@a` completes to `@all`
 * and the ranking is the whole of why.
 *
 * The handle is selectable only while it resolves to at least one agent
 * (AGT-FR-38); when it resolves to none it still renders, with the reason, on the
 * same terms AGT-FR-26 sets for an agent that is present but cannot answer — an
 * author reaching for it learns the project has nobody to hear it rather than
 * finding it silently absent.
 */
export function matchingEntries(agents: ProjectAgent[], query: string): MentionEntry[] {
  const needle = query.trim().toLowerCase();
  const out: MentionEntry[] = [];
  if (ALL_HANDLE.startsWith(needle)) {
    const ready = agents
      .filter((a) => a.availability === "ready")
      .map((a) => a.agent.nickname)
      .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));
    out.push({ kind: "all", nicknames: ready, selectable: ready.length > 0 });
  }
  for (const enrolled of matchingAgents(agents, query)) {
    out.push({
      kind: "agent",
      enrolled,
      selectable: enrolled.availability === "ready",
    });
  }
  return out;
}

/**
 * What the `@all` row shows where an agent's row shows its model: the nicknames
 * it will tag, so an author reads whom they are about to address before
 * addressing them (AGT-FR-39).
 *
 * Bounded to the one line every entry occupies, with a count standing in for the
 * rest — a project enrolling a dozen agents would otherwise push the row past the
 * picker, which is bounded by the composer it is anchored to.
 */
export function allHandleSummary(nicknames: string[], limit = 3): string {
  if (nicknames.length === 0) return "no agent here can answer";
  const shown = nicknames.slice(0, limit).map((n) => `@${n}`);
  const rest = nicknames.length - shown.length;
  return rest > 0 ? `${shown.join(", ")}, +${rest}` : shown.join(", ");
}

export function MentionPicker({
  agents,
  query,
  highlight,
  placement = "above",
  maxHeight = PICKER_MAX_HEIGHT,
  boxRef,
  onChoose,
}: MentionPickerProps) {
  const shown = matchingEntries(agents, query);
  return (
    <div
      ref={boxRef}
      className="card"
      role="listbox"
      aria-label="Agents"
      data-testid="mention-picker"
      data-placement={placement}
      // A nickname and a provider-qualified model id are both author-supplied
      // and both routinely long, so the panel is bounded by the composer it is
      // anchored to rather than by its widest entry — an unbounded one runs past
      // the rail, which clips it, and the model line disappears entirely.
      style={{
        position: "absolute",
        ...(placement === "above"
          ? { bottom: `calc(100% + ${PICKER_GAP}px)` }
          : { top: `calc(100% + ${PICKER_GAP}px)` }),
        left: 0,
        minWidth: 200,
        maxWidth: "100%",
        maxHeight,
        // So `maxHeight` means the space the picker occupies rather than the
        // space its rows occupy — the composer clamps it against the room its
        // clipping ancestor leaves, and a content-box height would overshoot
        // that by the padding and borders every time the clamp bound.
        boxSizing: "border-box",
        overflowY: "auto",
        zIndex: 30,
        padding: 4,
      }}
    >
      {/* AGT-FR-33: a fragment matching neither a nickname nor the handle
          renders a first-class row saying so rather than an empty box. */}
      {shown.length === 0 && (
        <div
          className="t-ui-sm t-muted"
          data-testid="mention-picker-empty"
          style={{ padding: "6px 8px" }}
        >
          No agent matches “{query}”.
        </div>
      )}
      {shown.map((entry, i) => {
        // AGT-FR-26 / AGT-FR-39: an entry that cannot answer — a degraded agent,
        // or the handle in a project where nobody can — is offered with the
        // reason stated and is not selectable, so an author looking for it learns
        // why rather than finding it silently absent.
        const blocked = !entry.selectable;
        const handle = entryHandle(entry);
        return (
          <button
            key={entry.kind === "all" ? "@all" : entry.enrolled.agent.id}
            role="option"
            aria-selected={i === highlight}
            aria-disabled={blocked}
            disabled={blocked}
            className="btn btn--ghost btn--sm"
            data-testid="mention-picker-option"
            data-entry={entry.kind}
            data-highlighted={i === highlight}
            style={{
              width: "100%",
              justifyContent: "space-between",
              gap: 8,
              overflow: "hidden",
              background: i === highlight ? "var(--bg-active)" : undefined,
              opacity: blocked ? 0.6 : 1,
            }}
            // AGT non-functional: the picker takes no focus away from the
            // composer, so a pointer-down on an entry must not blur it.
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => {
              if (!blocked) onChoose(handle);
            }}
          >
            {/* The tag is what the row is *for*, so it never yields width to
                what merely describes it. Both spans shrinking together — the
                flex default — divides the shortfall proportionally and cuts a
                four-character handle to `@a…` beside a forty-character summary
                that kept most of its own; the entry Tab is about to complete then
                does not legibly say what it inserts. The describing span carries
                the ellipsis instead. The cap is what keeps that from inverting on
                a nickname long enough to eat the whole row: past it the tag
                truncates too, so the row always shows something of both. */}
            <span
              className="u-ellipsis"
              style={{ flexShrink: 0, maxWidth: "70%" }}
            >
              @{handle}
            </span>
            <span className="t-ui-sm t-muted u-ellipsis" style={{ minWidth: 0 }}>
              {entry.kind === "all"
                ? // AGT-FR-39: the names it will tag, where an agent's row
                  // carries its title.
                  allHandleSummary(entry.nicknames)
                : blocked
                  ? availabilityShort(entry.enrolled.availability)
                  : // AGT-FR-43: the agent's title, and nothing in its place
                    // when there is none — no placeholder, no `Not defined`,
                    // and never the model, which this picker does not show at
                    // all. An empty span still holds the row's shape.
                    entry.enrolled.agent.title}
            </span>
          </button>
        );
      })}
    </div>
  );
}


// ---------------------------------------------------------------------------
// The composer the picker attaches to (AGT-FR-25, AGT-FR-27, AGT-FR-30)
// ---------------------------------------------------------------------------

export interface MentionComposerProps {
  value: string;
  onChange: (value: string) => void;
  /** The project's enrolled agents. An empty list renders a plain composer. */
  agents: ProjectAgent[];
  ariaLabel: string;
  placeholder?: string;
  disabled?: boolean;
  rows?: number;
  className?: string;
  testId?: string;
  autoFocus?: boolean;
  /**
   * CMT-FR-32 / AGT-FR-30: the picker is a transient surface of the surface that
   * owns the composer, so opening one of that surface's *other* transient
   * surfaces dismisses it. The owner raises this counter to say so.
   */
  dismissSignal?: number;
  /**
   * CMT-FR-45: pasting image data into a comment composer attaches it. Handled
   * by the owner rather than here, because what a paste means differs by
   * surface — this composer is also used where a paste is only ever text.
   */
  onPaste?: (event: React.ClipboardEvent<HTMLTextAreaElement>) => void;
  /**
   * A key the surface owning this composer wants, offered only while the picker
   * is **closed** (`CVP-conversation-presentation.md` CVP-FR-40).
   *
   * That is the whole of "preserves the mention-picker's own key handling": with
   * the picker open every key the picker claims is the picker's, so Enter
   * completes a nickname where it is open and the owner's accelerator posts where
   * it is not. Nothing here changes what reaches the text.
   */
  onKeyDown?: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  /**
   * Grow with what is typed, from one line up to `maxRows` and no further.
   *
   * A comment composer starts at a single line because that is what most replies
   * are, and a permanently three-line box spends the surface's width and height
   * on nothing. Past the ceiling it scrolls within itself rather than pushing the
   * conversation off the surface — a composer that can grow without bound takes
   * the whole overlay from the messages it is answering.
   */
  autoGrow?: boolean;
  maxRows?: number;
}

/**
 * A composer that offers the project's enrolled agents through the mention
 * picker of AGT-FR-25.
 *
 * Three properties the requirements turn on, all of them about not costing the
 * author their sentence:
 *
 * - **The picker takes no focus** (AGT non-functional). It is rendered beside
 *   the textarea rather than inside a focus trap, and an entry's pointer-down is
 *   prevented, so the caret never leaves the message.
 * - **Every key that does not choose an entry reaches the text** (AGT-FR-27).
 *   Only the arrows, Enter, Tab, and Escape are intercepted, and only while the
 *   picker is open with something to complete — a Tab with nothing highlighted
 *   moves focus as it otherwise would rather than stranding the author in a
 *   composer that has swallowed their key.
 * - **Escape leaves what was typed exactly as it was** (AGT-FR-27). Closing is
 *   local state; it rewrites nothing.
 */
export function MentionComposer({
  value,
  onChange,
  agents,
  ariaLabel,
  placeholder,
  disabled,
  rows,
  className,
  testId,
  autoFocus,
  dismissSignal,
  onPaste,
  onKeyDown,
  autoGrow = false,
  maxRows = 5,
}: MentionComposerProps) {
  const ref = useRef<HTMLTextAreaElement | null>(null);
  const [mention, setMention] = useState<{ query: string; start: number } | null>(
    null,
  );
  // AGT-FR-33: the index of the top match, or -1 when there is no selectable
  // entry at all. A "nothing is highlighted" state is load-bearing rather than
  // cosmetic: AGT-FR-26 keeps the highlight off an agent that cannot answer, and
  // AGT-FR-27 reads exactly this to decide that Tab has nothing to complete and
  // must therefore be left alone.
  const [highlight, setHighlight] = useState(-1);
  // AGT-FR-27: Escape closes the picker and leaves the text alone. Reopening is
  // typing another `@`, so a dismissal has to survive the keystrokes that
  // follow — hence a remembered offset rather than simply recomputing.
  const [dismissedAt, setDismissedAt] = useState<number | null>(null);
  /**
   * Where the caret belongs after an insertion. Applied in a layout effect
   * rather than a frame callback: `requestAnimationFrame` runs *after* the next
   * keystroke has already been dispatched, so a fast typist would have the caret
   * yanked back into the middle of what they were writing.
   */
  const [pendingCaret, setPendingCaret] = useState<number | null>(null);
  useLayoutEffect(() => {
    if (pendingCaret === null) return;
    const el = ref.current;
    setPendingCaret(null);
    if (!el) return;
    el.focus();
    el.setSelectionRange(pendingCaret, pendingCaret);
  }, [pendingCaret]);

  useEffect(() => {
    if (autoFocus) ref.current?.focus();
  }, [autoFocus]);

  /**
   * Re-measure whenever the value changes, which covers typing, pasting, a
   * completed mention, and the composer being restored with text already in it
   * from a presentation instance (CVP-FR-47).
   *
   * A layout effect so the height is settled before paint: measured in an
   * ordinary effect, a paste of five lines renders one line tall for a frame and
   * visibly jumps.
   */
  useLayoutEffect(() => {
    const el = ref.current;
    if (!autoGrow || !el) return;
    // jsdom reports no layout at all, so every measurement below is 0 and the
    // box would collapse. Left alone there, where the rendered height is not
    // something a test can observe anyway.
    if (typeof window === "undefined" || el.scrollHeight === 0) return;
    const styles = window.getComputedStyle(el);
    const lineHeight = Number.parseFloat(styles.lineHeight);
    const padding =
      Number.parseFloat(styles.paddingTop) +
      Number.parseFloat(styles.paddingBottom);
    // `scrollHeight` counts the content box and its padding but never the
    // border, while a `border-box` height counts all three — so the border is
    // added back or every composer renders two pixels short of its own text.
    const border =
      Number.parseFloat(styles.borderTopWidth) +
      Number.parseFloat(styles.borderBottomWidth);
    if (!Number.isFinite(lineHeight) || !Number.isFinite(padding)) return;
    const ceiling = lineHeight * maxRows + padding + border;
    // Reset first: `scrollHeight` never shrinks below the element's own height,
    // so a composer that has been emptied would keep the height it grew to.
    el.style.height = "auto";
    const content = el.scrollHeight + border;
    el.style.height = `${Math.min(content, ceiling)}px`;
    // Past the ceiling it scrolls within itself; below it there is nothing to
    // scroll and the bar would flicker in and out as the author types.
    el.style.overflowY = content > ceiling ? "auto" : "hidden";
  }, [autoGrow, maxRows, value]);

  useEffect(() => {
    if (dismissSignal === undefined) return;
    setMention(null);
  }, [dismissSignal]);

  /**
   * Which side of the composer the picker opens on. Above by preference, but a
   * card sitting high in the comment rail has less room above it than the picker
   * needs, and the rail clips what overflows it — so a picker that insisted on
   * opening upward would hide the very entry Tab is about to complete
   * (AGT-FR-27, AGT-FR-33).
   */
  const boxRef = useRef<HTMLDivElement | null>(null);
  const [fit, setFit] = useState<{ placement: "above" | "below"; maxHeight: number }>({
    placement: "above",
    maxHeight: PICKER_MAX_HEIGHT,
  });
  useLayoutEffect(() => {
    const el = ref.current;
    const box = boxRef.current;
    if (!el || !box) return;
    const anchor = el.getBoundingClientRect();
    // Nothing is laid out (jsdom, or a composer in a hidden tab): measuring
    // would only produce a confident wrong answer, so keep what we have.
    if (anchor.height === 0) return;
    const clip = clippingAncestor(el)?.getBoundingClientRect();
    const roomAbove = anchor.top - (clip?.top ?? 0);
    const roomBelow = (clip?.bottom ?? window.innerHeight) - anchor.bottom;
    // The *content's* height, not the rendered box's — the rendered one is
    // already clamped by whatever `maxHeight` this effect last chose, so
    // measuring it would feed the clamp back into the decision that produced it.
    // `scrollHeight` carries the padding but not the borders, and the comparison
    // below is against room in the page, so the borders have to be added back or
    // the picker is consistently that much taller than it thinks it is.
    const borders = box.offsetHeight - box.clientHeight;
    const natural = (box.scrollHeight || PICKER_MAX_HEIGHT) + borders;
    const wanted = Math.min(natural, PICKER_MAX_HEIGHT);
    const placement =
      roomAbove >= wanted + PICKER_GAP || roomAbove >= roomBelow ? "above" : "below";
    // On a window too short for either side, the picker scrolls within the room
    // it has rather than spilling past the edge that clips it — a shorter list
    // the author can scroll beats a taller one whose top entry is invisible.
    // Deliberately no minimum: a floor larger than the room available is just
    // the original clipping bug wearing a different number.
    const maxHeight = Math.max(
      0,
      Math.min(wanted, (placement === "above" ? roomAbove : roomBelow) - PICKER_GAP),
    );
    setFit((f) =>
      f.placement === placement && Math.abs(f.maxHeight - maxHeight) < 1
        ? f
        : { placement, maxHeight },
    );
  });

  const shown = mention ? matchingEntries(agents, mention.query) : [];
  /**
   * AGT-FR-25 / AGT-FR-39: the picker shows while there is something to offer.
   *
   * An enrolled agent is one such thing. The other is the `@all` handle, which
   * AGT-FR-38 has resolving to nobody in a project enrolling none and AGT-FR-39
   * still offers in its position with the reason stated — an author reaching for
   * it should learn the project has nobody to hear it rather than find it
   * silently absent. So a fragment that is a prefix of `all` opens the picker
   * even with an empty roster, while any other fragment in that project opens
   * nothing: someone who has enrolled no agent and is typing `@arch` or an email
   * address has asked for nothing and is shown nothing.
   */
  const open =
    mention !== null &&
    (agents.length > 0 || ALL_HANDLE.startsWith(mention.query.trim().toLowerCase()));
  /**
   * AGT-FR-26 / AGT-FR-39: an entry that cannot answer is offered with its reason
   * but is not selectable, so the arrows walk past it rather than parking on it.
   * Landing there would make the first ArrowDown in a roster whose second entry
   * is degraded do nothing at all.
   */
  const step = (from: number, delta: number): number => {
    if (shown.length === 0) return -1;
    // From "nothing highlighted", ArrowDown should reach the first entry and
    // ArrowUp the last, which is what anchoring the walk just outside the
    // corresponding end does.
    const base = from === -1 ? (delta > 0 ? -1 : 0) : from;
    for (let i = 1; i <= shown.length; i += 1) {
      const next = (base + delta * i + shown.length * i) % shown.length;
      if (shown[next]?.selectable) return next;
    }
    return from;
  };

  const recompute = (text: string, caret: number) => {
    const found = activeMention(text, caret);
    // AGT-FR-34: an Escape keeps the picker closed only while the caret is still
    // inside the candidate tag it dismissed. Once the caret leaves — a space, a
    // click elsewhere, a fresh sigil — the dismissal is spent, so typing `@` at
    // that same offset later opens the picker again rather than finding it
    // permanently poisoned.
    if (!found || found.start !== dismissedAt) {
      if (dismissedAt !== null) setDismissedAt(null);
    }
    if (!found || found.start === dismissedAt) {
      setMention(null);
      return;
    }
    setMention(found);
    // The highlight returns to the top only when the *query* moved, so an arrow
    // key — which changes the caret and nothing else — does not undo itself.
    if (found.query !== mention?.query || found.start !== mention?.start) {
      // AGT-FR-33: the top match is the first *selectable* entry of the list as
      // it renders — the `@all` handle where the fragment matches it (AGT-FR-39),
      // otherwise the leading nickname. A degraded agent that ranks above it is
      // passed over rather than highlighted, and a fragment whose every match is
      // degraded highlights nothing at all.
      const narrowed = matchingEntries(agents, found.query);
      setHighlight(narrowed.findIndex((e) => e.selectable));
    }
  };

  /**
   * Set when a completion was made with a key, and consumed by that same key's
   * `keyup`.
   *
   * A completion made mid-sentence leaves the caret at the end of a tag that is
   * *still* a candidate, so the `keyup` that follows the completing `keydown`
   * would reopen the picker on the nickname just inserted. Suppressing exactly
   * that one event is the whole fix, and it has to be a ref rather than state so
   * it is already true when the `keyup` handler reads it.
   *
   * Deliberately not a remembered offset: a *pointer* completion fires no
   * `keyup` at all, so a sticky suppression would never be spent and the author
   * could never delete back into that tag to correct it (AGT-FR-34).
   */
  const completedByKey = useRef(false);

  /**
   * Insert the chosen entry's tag. `handle` is a nickname or the `@all` handle —
   * AGT-FR-40 has the composer read `@all` rather than the nicknames it stands
   * for, so the handle goes in as itself and the expansion is never frozen into
   * the message.
   */
  const choose = (handle: string, viaKey = false) => {
    const el = ref.current;
    if (!el || !mention) return;
    const caret = el.selectionStart ?? value.length;
    const next = applyMention(value, caret, mention.start, handle);
    onChange(next.body);
    setMention(null);
    completedByKey.current = viaKey;
    // The caret belongs after the tag: the author is mid-sentence.
    setPendingCaret(next.caret);
  };

  return (
    <div
      /* The composer's own element rather than the textarea inside it. A flex
         parent sizes THIS, so it is what has to be told to take the width left
         over — a rule on the textarea reaches a box the wrapper has already
         sized to its intrinsic `cols`, which is how a 454px field ended up with
         a 140px message column. */
      className="mention-composer"
      style={{ position: "relative" }}
      /* AGT-FR-30 / CVP-FR-44: a surface Escape would otherwise dismiss — a
         detached conversation overlay — reads this to know the picker has the
         keystroke. Published as state on the element rather than by swallowing
         the event, because the Escape must still reach a comment card, which
         wants it for its own transient surfaces (CMT-FR-32). */
      data-mention-open={open ? "true" : undefined}
    >
      <textarea
        ref={ref}
        className={className}
        aria-label={ariaLabel}
        placeholder={placeholder}
        value={value}
        rows={autoGrow ? 1 : rows}
        /* The stylesheet gives a comment composer a floor and a resize grip,
           both of which fight a box that sizes itself to its text. */
        data-autogrow={autoGrow ? "true" : undefined}
        disabled={disabled}
        onPaste={onPaste}
        data-testid={testId}
        onChange={(e) => {
          onChange(e.target.value);
          recompute(e.target.value, e.target.selectionStart ?? e.target.value.length);
        }}
        onKeyUp={(e) => {
          // Arrow keys move the caret without changing the text, in both
          // directions: out of a candidate tag, which closes the picker, and
          // back into one, which offers it again. Deliberately ungated on
          // whether the picker is currently open — gating it there would make
          // arrowing back into a fragment do nothing while clicking at the very
          // same offset worked, which is exactly the pointer dependence the
          // keyboard-alone requirement rules out.
          //
          // The one event skipped is the `keyup` of the key that just completed
          // a tag, which would otherwise reopen the picker on the nickname the
          // author has this moment chosen.
          if (completedByKey.current) {
            completedByKey.current = false;
            return;
          }
          const el = e.currentTarget;
          recompute(el.value, el.selectionStart ?? el.value.length);
        }}
        // AGT-FR-34: a caret that has left the composer is not sitting at the end
        // of a candidate tag, so the picker goes with it rather than lingering
        // over a composer the author has tabbed away from.
        onBlur={() => setMention(null)}
        onClick={(e) => {
          const el = e.currentTarget;
          recompute(el.value, el.selectionStart ?? el.value.length);
        }}
        onKeyDown={(e) => {
          if (!open) {
            onKeyDown?.(e);
            return;
          }
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setHighlight((h) => step(h, 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setHighlight((h) => step(h, -1));
          } else if (e.key === "Enter") {
            // Prevented unconditionally while the picker is open. An Enter that
            // reached the textarea would put a newline into the message the
            // author is writing — which is what happens if the guard below is
            // allowed to decide whether to prevent it.
            e.preventDefault();
            const entry = shown[highlight];
            // AGT-FR-26 / AGT-FR-39: an entry that cannot answer is not
            // selectable, so Enter on it does nothing rather than inserting a tag
            // that reaches nobody.
            if (entry?.selectable) {
              choose(entryHandle(entry), true);
            }
          } else if (e.key === "Tab" && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
            // AGT-FR-27: Tab completes the highlighted entry exactly as Enter
            // does, so autocompleting is one key from where the author's hands
            // already are and the two keys never disagree about what the list
            // shows highlighted.
            const entry = shown[highlight];
            if (entry?.selectable) {
              e.preventDefault();
              choose(entryHandle(entry), true);
            }
            // With nothing to complete — the fragment matches neither a nickname
            // nor the handle, or everything it matches is an agent that cannot
            // answer or a handle resolving to none — Tab is deliberately *not*
            // prevented. Swallowing it would strand the author's focus in a
            // composer whose picker is showing a row that says there is nothing
            // there.
          } else if (e.key === "Escape") {
            e.preventDefault();
            // Not `stopPropagation`: the surface owning this composer may also
            // want the Escape, and swallowing it would strand a card open.
            setDismissedAt(mention?.start ?? null);
            setMention(null);
          }
        }}
      />
      {open && (
        <MentionPicker
          agents={agents}
          query={mention.query}
          highlight={highlight}
          placement={fit.placement}
          maxHeight={fit.maxHeight}
          boxRef={boxRef}
          onChoose={choose}
        />
      )}
    </div>
  );
}
