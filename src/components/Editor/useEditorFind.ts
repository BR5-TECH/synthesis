/**
 * Find and Find & Replace over the Editor's surfaces
 * (`../../../specifications/ui/EFR-editor-find-replace.md`).
 *
 * Split out of the Editor whole — its state, its match engine, the focus and
 * scroll effects, and the two replace actions together — because they are one
 * mechanism: the engine numbers matches in the order the surfaces render, the
 * decorations mark the set it produced, and Replace rewrites exactly that set.
 * A piece of it living elsewhere is a piece that can disagree with the counter.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { getMarkdown } from "../markdownFidelity";
import { useFind, type FindTarget, type SurfaceMatch } from "../../hooks/useFind";
import { useSourceTokens } from "../../hooks/useSourceTokens";
import { EditSessionStore } from "../../state/editSessions";
import type { EditMode, EditSurface } from "../../state/editHistory";
import type { EditSession } from "../../state/editSessions/types";
import { sealBurst } from "../../state/editHistory";
import { applyReplacements, type FindMatch } from "../../state/findMatches";
import type { FindForm } from "../../state/findState";
import {
  CURRENT_MATCH_CLASS,
  docText,
  findHighlightKey,
  rangeToPositions,
  type DocText,
  type PosRange,
} from "../findHighlight";

/** An empty projection, so a closed panel indexes nothing. */
const EMPTY_DOC_TEXT: DocText = { text: "", segments: [] };

export interface EditorFindDeps {
  artifactId: string;
  sessions: EditSessionStore;
  session: EditSession;
  /** EXC-FR-VTUH / EXC-FR-QDMC: a modal is blocking the tab. */
  blocked: boolean;
  mode: EditMode;
  isMarkdown: boolean;
  editor: TiptapEditor | null;
  sourceText: string;
  sourceTextRef: RefObject<string>;
  fm: string | null;
  fmRef: RefObject<string | null>;
  fmExpanded: boolean;
  bodyRef: RefObject<string>;
  replacingRef: RefObject<boolean>;
  rootRef: RefObject<HTMLDivElement | null>;
  docVersion: number;
  currentDoc: () => string;
  noteEdit: (doc: string, mode: EditMode, surface: EditSurface) => void;
  applyFm: (next: string | null) => void;
  applySourceText: (next: string) => void;
  setExpandOverride: (next: boolean | null) => void;
}

export function useEditorFind(deps: EditorFindDeps) {
  const {
    artifactId,
    sessions,
    session,
    blocked,
    mode,
    isMarkdown,
    editor,
    sourceText,
    sourceTextRef,
    fm,
    fmRef,
    fmExpanded,
    bodyRef,
    replacingRef,
    rootRef,
    docVersion,
    currentDoc,
    noteEdit,
    applyFm,
    applySourceText,
    setExpandOverride,
  } = deps;

  // ---- Find & Find & Replace (`EFR-editor-find-replace.md`) ------------------------

  const find = session.find;
  /**
   * EFR-FR-HLGY: while a modal blocks the tab both panels are inert along with
   * every other edit operation — nothing is matched or highlighted and the
   * replace actions have nothing to act on. The panel stays rendered so the
   * query is not lost behind the modal.
   */
  const findActive = find.form !== null && !blocked;

  /**
   * ESH-FR-XXYW: the source file's syntax tokens, or `null` while it renders
   * plain — a Markdown file's raw-text mode, a file no language resolves for,
   * and the moment between an edit and the pass that describes it (ESH-FR-MJRH).
   */
  const sourceTokens = useSourceTokens(
    artifactId,
    sourceText,
    mode === "text" && !isMarkdown && !blocked,
  );
  /**
   * Whether the layer behind the textarea is drawn.
   *
   * All three of its reasons share it, so a query matched inside a coloured
   * token keeps both its marking and its colour (ESH-FR-GXUX). A source file
   * carries it whether or not it has tokens: it is also where the comment
   * anchors are marked and measured (CMT-FR-27, CMT-FR-28), which a textarea
   * cannot do — and a layer of plain text over plain text is the same glyphs in
   * the same place.
   *
   * Except while the external-change modal blocks the tab (EXC-FR-VTUH), where the
   * surface is inert and says so by rendering as disabled. The layer draws its
   * text in the ordinary foreground, so leaving it up would show a live-looking
   * document over a surface that accepts nothing.
   */
  const sourceLayer = (!isMarkdown && !blocked) || findActive;

  const sourceRef = useRef<HTMLTextAreaElement>(null);
  const sourceHlRef = useRef<HTMLPreElement>(null);
  /** The source page's own box — what the affordance is positioned within. */
  const sourceWrapRef = useRef<HTMLDivElement>(null);
  const fmInputRef = useRef<HTMLTextAreaElement>(null);
  const [findFocus, setFindFocus] = useState<"query" | "replacement">("query");

  /**
   * The rich body projected to flat text, with the index back to ProseMirror
   * positions (EFR-FR-DSKI). Built only while a panel is open: the editor
   * re-renders on every selection move, and with the panel closed there is
   * nothing to match against.
   */
  const bodyIndex: DocText = useMemo(
    () =>
      findActive && editor && mode === "wysiwyg"
        ? docText(editor.state.doc)
        : EMPTY_DOC_TEXT,
    // `docVersion` stands in for the document itself, which is mutable.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [findActive, editor, mode, docVersion, session.seedToken],
  );

  /**
   * EFR-FR-DDUX: the surfaces the query runs over — the raw-text source alone, or
   * the frontmatter region followed by the rich body, in the order they render,
   * which is the order the counter numbers their matches in.
   */
  const findTargets: FindTarget[] = useMemo(() => {
    if (!findActive) return [];
    if (mode === "text") return [{ id: "source", text: sourceText }];
    const targets: FindTarget[] = [];
    if (fm !== null) targets.push({ id: "frontmatter", text: fm });
    targets.push({ id: "body", text: bodyIndex.text });
    return targets;
  }, [findActive, mode, sourceText, fm, bodyIndex]);

  /**
   * A body match is only usable if it resolves to one contiguous span of the
   * document: the flat projection joins textblocks with a newline and skips
   * inline nodes, so a match straddling either has no single range to highlight
   * or rewrite. Dropping it here keeps the counter, the decorations and Replace
   * describing the same set (EFR-FR-DXTV, EFR-FR-FWOU).
   */
  const usableMatch = useCallback(
    (m: SurfaceMatch) =>
      m.surface !== "body" || rangeToPositions(bodyIndex, m.start, m.end) !== null,
    [bodyIndex],
  );

  const engine = useFind(
    findTargets,
    find.query,
    find.mode,
    findActive,
    usableMatch,
  );

  const patchFind = useCallback(
    (patch: Parameters<EditSessionStore["setFind"]>[1]) => {
      sessions.setFind(artifactId, patch);
    },
    [artifactId, sessions],
  );

  /**
   * Where focus goes when the panel closes (EFR-FR-CELO): the current match, kept
   * resolved while the panel is open because closing it empties the match set
   * and tears down the body's position index along with it.
   */
  const focusTargetRef = useRef<{
    surface: SurfaceMatch["surface"];
    start: number;
    end: number;
    pos: PosRange | null;
  } | null>(null);
  useEffect(() => {
    if (!findActive) return;
    const m = engine.currentMatch;
    focusTargetRef.current = m
      ? {
          surface: m.surface,
          start: m.start,
          end: m.end,
          pos:
            m.surface === "body"
              ? rangeToPositions(bodyIndex, m.start, m.end)
              : null,
        }
      : null;
  }, [findActive, engine.currentMatch, bodyIndex]);

  /**
   * EFR-FR-BBKS/EFR-FR-BSST/EFR-FR-CELO: react to the panel's form changing.
   *
   * Opening focuses the query input; expanding Find into Find & Replace focuses
   * the replacement input instead. Closing returns focus to the editing surface,
   * placed at the current match so the user resumes where the search left them.
   */
  const prevFormRef = useRef<FindForm | null>(find.form);
  useEffect(() => {
    const prev = prevFormRef.current;
    prevFormRef.current = find.form;
    if (find.form !== null) {
      setFindFocus(
        prev === "find" && find.form === "replace" ? "replacement" : "query",
      );
      return;
    }
    if (prev === null) return; // already closed; nothing was dismissed
    const target = focusTargetRef.current;
    // With no current match — an empty query, or nothing found — focus still
    // returns to the editing surface, just without a selection to place.
    if (!target) {
      if (sourceRef.current) sourceRef.current.focus();
      else editor?.commands.focus();
      return;
    }
    if (target.surface === "source") {
      const el = sourceRef.current;
      el?.focus();
      el?.setSelectionRange(target.start, target.end);
    } else if (target.surface === "frontmatter") {
      // The region mounts no textarea while collapsed, and expanding it is a
      // state change that has not committed yet — so the caret is placed once
      // the expanded region is on screen rather than against a null ref.
      setExpandOverride(true);
      setPendingFmSelection({ start: target.start, end: target.end });
    } else if (editor && target.pos) {
      editor.chain().setTextSelection(target.pos).focus().run();
    }
  }, [find.form, editor]);

  /**
   * Place a caret in the frontmatter region once it has actually mounted, for
   * the close-the-panel-on-a-frontmatter-match path above (EFR-FR-CELO).
   */
  const [pendingFmSelection, setPendingFmSelection] = useState<{
    start: number;
    end: number;
  } | null>(null);
  useEffect(() => {
    if (!pendingFmSelection) return;
    const el = fmInputRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(pendingFmSelection.start, pendingFmSelection.end);
    setPendingFmSelection(null);
  }, [pendingFmSelection, fmExpanded]);

  /**
   * EFR-FR-DSKI: a current match inside a collapsed frontmatter region expands it
   * as a peek, so the match is visible where it was found — the same principle
   * as an undo that reaches into it (EDT-FR-25).
   */
  const currentSurface = engine.currentMatch?.surface;
  useEffect(() => {
    if (currentSurface === "frontmatter") setExpandOverride(true);
  }, [currentSurface, engine.current]);

  /**
   * EFR-FR-DOQR: push the body's matches into the decoration plugin. A meta-only
   * transaction changes no document, so this adds no history step (EDT-FR-23)
   * and does not re-enter `onUpdate`.
   */
  const bodyMatches = useMemo(
    () => engine.matches.filter((m) => m.surface === "body"),
    [engine.matches],
  );

  const bodyRanges: PosRange[] = useMemo(() => {
    if (mode !== "wysiwyg") return [];
    // Every body match is resolvable — `usableMatch` dropped the ones that are
    // not — so this maps one-for-one and `currentBodyRange` below indexes the
    // same list the plugin decorates.
    const out: PosRange[] = [];
    for (const m of bodyMatches) {
      const pos = rangeToPositions(bodyIndex, m.start, m.end);
      if (pos) out.push(pos);
    }
    return out;
  }, [bodyMatches, bodyIndex, mode]);

  // Index of the current match *within the body's* ranges, which is what the
  // plugin distinguishes; -1 when the current match is elsewhere or absent.
  const currentBodyRange = useMemo(() => {
    const m = engine.currentMatch;
    if (!m || m.surface !== "body") return -1;
    return bodyMatches.findIndex((x) => x.start === m.start && x.end === m.end);
  }, [bodyMatches, engine.currentMatch]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const tr = editor.state.tr.setMeta(findHighlightKey, {
      ranges: bodyRanges,
      current: currentBodyRange,
    });
    tr.setMeta("addToHistory", false);
    editor.view.dispatch(tr);
  }, [editor, bodyRanges, currentBodyRange]);

  /**
   * EFR-FR-DOQR: bring the current match into view. Each surface scrolls its own
   * way — the source textarea is scrolled to the offset its highlight layer
   * reports, while the body and the frontmatter region live in the document's
   * own scroll container and can be scrolled to directly.
   */
  useEffect(() => {
    if (!findActive || engine.current < 0) return;
    const raf =
      typeof requestAnimationFrame === "function"
        ? requestAnimationFrame
        : (fn: FrameRequestCallback) => setTimeout(() => fn(0), 0);
    raf(() => {
      const surface = engine.currentMatch?.surface;
      if (surface === "source") {
        const mark = sourceHlRef.current?.querySelector<HTMLElement>(
          `.${CURRENT_MATCH_CLASS}`,
        );
        const el = sourceRef.current;
        if (!mark || !el) return;
        const top = mark.offsetTop;
        const bottom = top + mark.offsetHeight;
        if (top < el.scrollTop) el.scrollTop = top;
        else if (bottom > el.scrollTop + el.clientHeight) {
          el.scrollTop = bottom - el.clientHeight;
        }
        return;
      }
      const root = rootRef.current;
      const mark = root?.querySelector<HTMLElement>(`.${CURRENT_MATCH_CLASS}`);
      mark?.scrollIntoView?.({ block: "nearest" });
    });
  }, [findActive, engine.current, engine.currentMatch, bodyRanges]);

  /**
   * EFR-FR-EYLX/EFR-FR-FYNZ: rewrite `targets` with the replacement text.
   *
   * Every surface the operation touches is rewritten first and the whole thing
   * then records exactly **one** step, so a Replace All across the frontmatter
   * and the body is a single undo however many occurrences it rewrote. The step
   * is sealed on both sides so neither the typing before it nor a Replace after
   * it can coalesce into it.
   */
  const replaceMatches = useCallback(
    (targets: readonly SurfaceMatch[]) => {
      if (blocked || targets.length === 0) return;
      const replacement = session.find.replacement;
      const before = currentDoc();
      const pick = (surface: SurfaceMatch["surface"]): FindMatch[] =>
        targets
          .filter((m) => m.surface === surface)
          .map(({ start, end }) => ({ start, end }));

      replacingRef.current = true;
      try {
        const inSource = pick("source");
        if (inSource.length > 0) {
          applySourceText(
            applyReplacements(sourceTextRef.current, inSource, replacement),
          );
        }
        const inFm = pick("frontmatter");
        if (inFm.length > 0 && fmRef.current !== null) {
          applyFm(applyReplacements(fmRef.current, inFm, replacement));
        }
        const inBody = pick("body");
        if (inBody.length > 0 && editor && !editor.isDestroyed) {
          const positions = inBody
            .map((m) => rangeToPositions(bodyIndex, m.start, m.end))
            .filter((p): p is PosRange => p !== null);
          if (positions.length > 0) {
            const tr = editor.state.tr;
            // Applied last-first so each rewrite leaves the positions of the
            // ones before it untouched, and all of them land in one transaction
            // — one document change, one history step.
            for (const p of [...positions].reverse()) {
              if (replacement === "") tr.delete(p.from, p.to);
              else tr.insertText(replacement, p.from, p.to);
            }
            editor.view.dispatch(tr);
            bodyRef.current = getMarkdown(editor);
          }
        }
      } finally {
        replacingRef.current = false;
      }

      const next = currentDoc();
      // A replacement that changed nothing is not an edit: it must not mark the
      // artifact dirty (EDT-FR-04) or occupy a position in the history
      // (EDT-FR-23). Reachable when the replacement text equals what it
      // replaces, and it is what keeps a no-op honest.
      if (next === before) return;
      sealBurst(session.history);
      noteEdit(next, session.mode, targets[0].surface);
      sealBurst(session.history);
    },
    [
      blocked,
      session,
      applySourceText,
      applyFm,
      editor,
      bodyIndex,
      currentDoc,
      noteEdit,
    ],
  );

  /**
   * EFR-FR-EXHA: Replace rewrites the current match and advances to the next one.
   * The anchor is moved past the inserted text first, so a replacement that
   * itself contains the query advances rather than landing on what it just
   * wrote.
   */
  const replaceCurrent = useCallback(() => {
    const m = engine.currentMatch;
    if (!m) return;
    engine.setAnchor(m.surface, m.start + session.find.replacement.length);
    replaceMatches([m]);
  }, [engine, replaceMatches, session]);

  const replaceAll = useCallback(() => {
    if (engine.matches.length === 0) return;
    engine.setAnchor(null, 0);
    replaceMatches(engine.matches);
  }, [engine, replaceMatches]);

  return {
    find,
    findActive,
    sourceTokens,
    sourceLayer,
    sourceRef,
    sourceHlRef,
    sourceWrapRef,
    fmInputRef,
    findFocus,
    engine,
    patchFind,
    replaceCurrent,
    replaceAll,
  };
}
