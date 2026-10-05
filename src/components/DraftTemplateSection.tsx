import { useCallback, useEffect, useRef, useState } from "react";
import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { AUTOSAVE_DELAY_MS } from "../state/writeSchedule";
import {
  TEMPLATE_DOC_KEY,
  draftTemplateTransport,
  registerDraftTemplateWrite,
} from "../state/draftTemplate";
import { registerSettingsSection } from "../state/settingsSweep";
import { logInfo, logWarn } from "../logging";

/**
 * SET-FR-18: where the author's text stands, in three states and no others.
 * `idle` is the fourth thing this component can be in and the one the section
 * never *reports*: nothing has been typed since the read landed, so there is
 * nothing to say about a write.
 */
type WriteStatus = "idle" | "saving" | "saved" | "failed";

interface DraftTemplateSectionProps {
  /**
   * SET-FR-20: what the section's template belongs to — the open project and its
   * active worktree. The section discards what it holds and reads again whenever
   * this changes, so it can neither show nor overwrite one project's template
   * while another project's is loading.
   */
  contentRoot: string;
}

/**
 * The **Draft template** section of the Project settings window (SET-FR-16 …
 * SET-FR-20).
 *
 * It holds exactly one thing: the project's optional Markdown draft template,
 * the starting content every draft created in this project is born holding
 * (DRS-FR-39). No name, no path, no artifact type, no destination, and no
 * per-draft setting — a template is one document and everything else about a
 * draft is decided in the draft.
 *
 * The editing surface is the Editor's own, reused whole rather than
 * reimplemented (SET-FR-16): the same two modes, the same frontmatter region,
 * the same formatting band, the same page framing and the same round-trip
 * fidelity. There is no second editor implementation and no plain text area
 * anywhere in here, so the surface's editing, serialisation, line-ending,
 * focus, and accessibility behaviour are unchanged by being used here.
 *
 * And there is no Save control (SET-FR-17): the template writes itself a short
 * rest after the last keystroke, and immediately when this section is left or
 * the window closes — so the section takes no part in the per-section dirty
 * state of SET-FR-08 and raises no discard-unsaved-changes confirmation
 * (SET-FR-09, SWN-FR-10). There is nothing to discard, because what the author
 * typed is already on its way to disk.
 */
export function DraftTemplateSection({
  contentRoot,
}: DraftTemplateSectionProps) {
  /**
   * SET-FR-20: the store the template is edited in, or `null` while a read is
   * outstanding.
   *
   * Built per read rather than once, so the incoming worktree's template is
   * adopted into a **fresh** session: reusing one would resume the outgoing
   * project's buffer, dirty flag and undo history against the new project's
   * store — which is the one way one project's template gets written into
   * another's.
   */
  const [store, setStore] = useState<EditSessionStore | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [status, setStatus] = useState<WriteStatus>("idle");
  /** Bumped on every edit the Editor reports, so the rest below restarts. */
  const [editTick, setEditTick] = useState(0);

  /**
   * The live store, for the unmount flush — which runs after the state that
   * held it has already been torn down.
   */
  const storeRef = useRef<EditSessionStore | null>(null);
  storeRef.current = store;

  /**
   * SWN-FR-09: the write currently in flight, or null.
   *
   * A settings write already in flight is one of the window's pending changes,
   * and the close **awaits** it rather than starting it again: the sweep issues
   * no second write for it, and the window closes once it lands. Without this,
   * a close arriving between the rest elapsing and the write landing would send
   * the same text twice — two whole-store writes racing each other over
   * `project.toml`.
   */
  const inFlight = useRef<Promise<boolean> | null>(null);

  /**
   * SET-FR-17 / SET-FR-18: perform the write and report where it landed.
   *
   * A failure leaves the author's content in the editor exactly as they left it
   * — the store's own write path neither reverts, reloads, nor replaces the
   * buffer (EDT-FR-71) — so a retry is a click rather than the text they wrote.
   */
  const save = useCallback(async (): Promise<boolean> => {
    // SWN-FR-09: a write already in flight is awaited rather than started
    // again. Every caller — the rest timer, the section being left, the
    // window's save-before-close sweep, and the author's own retry — funnels
    // through this one promise.
    if (inFlight.current) return inFlight.current;
    const live = storeRef.current;
    if (!live) return true;
    if (!live.ensure(TEMPLATE_DOC_KEY).dirty) return true;
    setStatus("saving");
    const run = (async () => {
      const res = await live.flush(TEMPLATE_DOC_KEY);
      if (res.ok) {
        setStatus("saved");
        const body = live.ensure(TEMPLATE_DOC_KEY).buffer;
        // SET-FR-19: an empty editor persists the *unset* state. Worth a record
        // of its own, because it changes what every draft created afterwards
        // opens on and the author is told nothing beyond "Saved". The template
        // is the author's own text, so its length says what happened without
        // carrying it.
        logInfo(["frontend"], "draft template saved", {
          cleared: body === "",
          length: body.length,
        });
        return true;
      }
      setStatus("failed");
      logWarn(["frontend"], "draft template write failed", {
        // The store put the transport's own message on the record; the bytes it
        // could not write are the author's content and stay out of the log.
        reason: live.ensure(TEMPLATE_DOC_KEY).error ?? res.blocked ?? "unknown",
      });
      return false;
    })();
    inFlight.current = run;
    try {
      return await run;
    } finally {
      if (inFlight.current === run) inFlight.current = null;
    }
  }, []);

  // SET-FR-20: read on mount, and again whenever the active project or the
  // project's active worktree changes. What the section was holding is dropped
  // first, so a loading state renders in place of the outgoing template rather
  // than the outgoing template staying on screen over the incoming project.
  useEffect(() => {
    let cancelled = false;
    setStore(null);
    setLoadError(null);
    setStatus("idle");
    void draftTemplateTransport
      .load(TEMPLATE_DOC_KEY)
      .then(({ body, checksum }) => {
        if (cancelled) return;
        const next = new EditSessionStore(draftTemplateTransport, false);
        // Adopted rather than left for the Editor's own first load to fetch, so
        // the read this section already made is the one the surface mounts on
        // — one `"load project config"` per read rather than two.
        next.adoptLoad(TEMPLATE_DOC_KEY, body, checksum);
        setStore(next);
      })
      .catch((e) => {
        if (cancelled) return;
        // SET-FR-18: a failed load states that the project's configuration
        // could not be read and edits nothing. It never presents an empty
        // editor as though the project configured no template, so a damaged
        // store is never mistaken for a cleared one — and, because no editor
        // mounts, never overwritten by what the section happened to be showing.
        setLoadError(String(e));
        logWarn(["frontend"], "draft template could not be read", {
          reason: String(e),
        });
      });
    return () => {
      cancelled = true;
      // SET-FR-17: whatever the outgoing project's editor still held is written
      // on the way out — not discarded with the store the line above is about
      // to replace. The cleanup runs on a `contentRoot` change as well as on an
      // unmount, so leaving the section, closing the tab, and switching project
      // or worktree all reach the same write. `save` reads the store through a
      // ref, and this cleanup runs before the next read installs a new one, so
      // it is the *outgoing* buffer that is written.
      void save();
    };
  }, [contentRoot, save]);

  /**
   * SET-FR-17: the template writes itself a short rest after the last
   * keystroke, and a further edit before the rest elapses restarts it — so a
   * burst of typing is one write carrying the settled text rather than one
   * write per character.
   */
  useEffect(() => {
    if (!store) return;
    if (!store.ensure(TEMPLATE_DOC_KEY).dirty) return;
    const timer = setTimeout(() => void save(), AUTOSAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [store, editTick, save]);

  /**
   * SET-FR-17: and a teardown brings the write forward and **waits** for it.
   *
   * A project or worktree change closes every tab (TAB-FR-14), which unmounts
   * this section — and the cleanup above cannot be awaited, so on its own it
   * would race the switch and land the write against the wrong content root, or
   * after the project had been closed under it. Publishing the write is what
   * lets the shell's teardown path spend it first, exactly as it spends every
   * artifact's, Flow's and draft's.
   */
  useEffect(() => registerDraftTemplateWrite(save), [save]);

  /**
   * SWN-FR-08 / SWN-FR-09: publish the template to the window's
   * save-before-close sweep.
   *
   * A write already in flight counts as pending exactly as a dirty buffer does
   * — the close waits for it and issues no second write for it (SWN-FR-09) —
   * and a failure cancels the close with this section presented and the
   * author's text untouched (SET-FR-18, SWN-FR-11).
   */
  useEffect(
    () =>
      registerSettingsSection({
        section: "template",
        pending: () =>
          inFlight.current !== null ||
          storeRef.current?.ensure(TEMPLATE_DOC_KEY).dirty === true,
        save,
      }),
    [save],
  );

  if (loadError) {
    return (
      <div className="card" style={{ padding: "12px 14px" }} role="alert">
        <div className="t-ui-sm" style={{ color: "var(--danger, #e5484d)" }}>
          This project's configuration could not be read.
        </div>
        <p className="t-ui-xs t-muted" style={{ margin: "6px 0 0" }}>
          {loadError}
        </p>
        <p className="t-ui-xs t-muted" style={{ margin: "6px 0 0" }}>
          Repair <code>.synthesis/project.toml</code> and open this section
          again. Nothing has been written from here.
        </p>
      </div>
    );
  }

  if (!store) {
    // SET-FR-20: the loading state, rather than any previously-read template.
    // No editing surface is mounted at all, so no edit can be taken — and none
    // can be written — until the read has landed.
    return (
      <div className="t-ui-sm t-muted" role="status">
        Loading the project's draft template…
      </div>
    );
  }

  return (
    // SWN-FR-03: fills the height the window's content column has rather than
    // taking a natural height that runs past a fold the author cannot resize
    // past — the editing surface is the section, so it should be given whatever
    // room the frame has.
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      <p className="t-ui-xs t-muted" style={{ margin: "0 0 10px" }}>
        Every draft created in this project starts with this text. Leave it
        empty for drafts that start blank. Changing it does not touch drafts
        that already exist.
      </p>
      {/* SET-FR-18: saving, saved, or a failure naming what went wrong — with a
          retry, so a transient failure costs a click rather than the text. */}
      <div
        className="t-ui-xs"
        role="status"
        data-testid="draft-template-status"
        style={{ margin: "0 0 8px", minHeight: 18 }}
      >
        {status === "saving" && <span className="t-muted">Saving…</span>}
        {status === "saved" && <span className="t-muted">Saved</span>}
        {status === "failed" && (
          <span style={{ color: "var(--danger, #e5484d)" }}>
            The draft template could not be saved.{" "}
            <button className="btn btn--ghost btn--sm" onClick={() => void save()}>
              Retry
            </button>
          </span>
        )}
      </div>
      {/* SET-FR-16: the Editor's own rich Markdown surface, reused whole. The
          comment rail is off — a template is not an item of the project and has
          no threads — and the action control with it: this is a section of a
          tab, and the tab's chrome belongs to the tab. */}
      <div style={{ flex: 1, minHeight: 180, display: "flex" }}>
        <Editor
          key={contentRoot}
          artifactId={TEMPLATE_DOC_KEY}
          artifactName="Draft template"
          sessions={store}
          showComments={false}
          showActions={false}
          onEdit={() => setEditTick((n) => n + 1)}
        />
      </div>
    </div>
  );
}
