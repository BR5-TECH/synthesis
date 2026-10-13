/**
 * Images in the prompt of a draft (`NAW-new-artifact.md` NAW-FR-50 …
 * NAW-FR-56). Split out of `index.tsx` to keep that file within the size rule.
 *
 * This hook holds what the tab lends the editing surface about images, the
 * immediate write an insertion takes, and the asset housekeeping trigger. It
 * renders nothing.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import { imageRefusal } from "./labels";

export function useDraftImages(input: {
  draftId: string;
  /** Whether the prompt's buffer holds an edit that is not written yet. */
  dirty: boolean;
  /** Bumped on every edit the Editor reports. */
  editTick: number;
  /** NAW-FR-13: write the pending buffer, and report whether it landed. */
  saveCurrent: () => Promise<boolean>;
}) {
  const { draftId, dirty, editTick, saveCurrent } = input;

  /**
   * NAW-FR-54: what the tab last announced about an image, read by a live
   * region so a screen-reader user learns that the image went in, that it was
   * refused and why, or that the reference is gone.
   *
   * One region rather than one per outcome: the three are the same kind of
   * thing to a reader, and a region per outcome would announce an empty string
   * on every re-render of the two that did not change.
   */
  const [imageStatus, setImageStatus] = useState("");
  /**
   * NAW-FR-51: the typed refusal of the last insertion, rendered inline where
   * the author is looking. Cleared by the next insertion, so a refusal that has
   * been answered does not stand over a picture that went in.
   */
  const [imageError, setImageError] = useState<string | null>(null);
  /**
   * NAW-FR-51: the asset an insertion has stored whose reference has not
   * reached the saved prompt yet.
   *
   * Held so the write that carries it can be made **immediately** rather than
   * after the ordinary rest, and so a write that fails can take the asset back
   * with `"discard draft image"` — a failed insertion leaves neither a
   * reference to a missing image nor an image nothing references.
   */
  const pendingInsertRef = useRef<string | null>(null);

  const announceImage = useCallback((message: string) => {
    setImageStatus(message);
  }, []);

  /**
   * NAW-FR-50 / NAW-FR-51: the tab's side of the editing surface's image offer.
   *
   * The tab takes the image data the surface offers it, stores it under the
   * draft's own `assets/` folder, and answers with the Markdown destination to
   * insert. What the surface then does with that answer is its own — one edit,
   * one undo step, one predictable selection (EDT-FR-86).
   *
   * A store that is **refused** answers with nothing at all, so no reference is
   * inserted, the prompt is byte-for-byte what it was, and no undo step is
   * taken. The refusals divide in what they tell the author: the three image
   * refusals say the picture was not acceptable, the lock says the draft takes
   * no write while its run holds it, and `asset_store_failed` says the store did
   * not happen and may simply be tried again.
   */
  const acceptImage = useCallback(
    async (image: { mediaType: string; filename: string | null; data: string }) => {
      setImageError(null);
      let asset;
      try {
        asset = await api.storeDraftImage(
          draftId,
          image.mediaType,
          image.filename,
          image.data,
        );
      } catch (e) {
        const message = imageRefusal(String(e));
        setImageError(message);
        announceImage(message);
        return null;
      }
      // NAW-FR-51: the prompt is written **immediately**, without waiting the
      // ordinary rest out, because the saved prompt is what protects the asset
      // from housekeeping (per `../../specifications/core/DAS-draft-assets.md`
      // DAS-FR-13). The write is scheduled for after the surface has applied
      // the insertion, which is why it is queued rather than awaited here.
      pendingInsertRef.current = asset.path;
      announceImage("Image inserted.");
      return `![](${asset.reference})`;
    },
    [draftId, announceImage],
  );

  /**
   * NAW-FR-52: resolve one of the prompt's image destinations to drawable
   * content, fetched when the image is actually shown rather than when the
   * prompt loads.
   *
   * A destination that resolves to no draft-owned asset — a missing file, an
   * unresolved reference-style label, an external address, a path leaving the
   * draft — answers `null`, and the surface renders its non-blocking
   * placeholder. Nothing here fetches an address.
   */
  const resolveImage = useCallback(
    async (destination: string) => {
      try {
        const content = await api.readDraftImage(draftId, destination);
        return `data:${content.mediaType};base64,${content.data}`;
      } catch {
        return null;
      }
    },
    [draftId],
  );

  const imageHost = useMemo(
    () => ({ resolve: resolveImage, accept: acceptImage }),
    [resolveImage, acceptImage],
  );

  /**
   * NAW-FR-51: an insertion writes the prompt **immediately**, without waiting
   * the ordinary rest of NAW-FR-13 out, because the saved prompt is what
   * protects the asset from housekeeping.
   *
   * Where that write fails the asset is discarded, so the failed insertion
   * leaves neither a reference to a missing image nor an image nothing
   * references. A discard that is itself refused changes none of that: the
   * saved prompt still carries no reference, the author still reads the write's
   * own failure and nothing about the cleanup, and the orphaned asset is left
   * for the next housekeeping pass (NAW-FR-55).
   */
  useEffect(() => {
    const path = pendingInsertRef.current;
    if (!path || !dirty) return;
    pendingInsertRef.current = null;
    void (async () => {
      if (await saveCurrent()) return;
      announceImage("The prompt could not be written. The image was not inserted.");
      try {
        await api.discardDraftImage(draftId, path);
      } catch {
        // DAS-FR-28: left for the next pass, and never reported to the author —
        // the failure they are owed is the write's, which they already have.
      }
    })();
  }, [editTick, dirty, saveCurrent, draftId, announceImage]);

  /**
   * NAW-FR-55: the draft's asset housekeeping, triggered when the draft opens
   * and when it closes — the close-triggered call made before the tab releases
   * the draft.
   *
   * A **trigger and not a query**: the answer is not read, not waited on, and
   * nothing is rendered from it. Neither call blocks anything, and a pass that
   * fails changes nothing about the open or the close — it raises no dialog,
   * marks nothing, and is reported through the application's own diagnostics
   * rather than to the author.
   */
  useEffect(() => {
    void api.sweepDraftAssets(draftId).catch(() => {});
    return () => {
      void api.sweepDraftAssets(draftId).catch(() => {});
    };
  }, [draftId]);

  return { imageStatus, imageError, imageHost, announceImage };
}
