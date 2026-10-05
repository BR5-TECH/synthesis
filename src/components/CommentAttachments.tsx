/**
 * Attachments in the comment rail (`CMT-comments.md` CMT-FR-45 … CMT-FR-51).
 *
 * Three things live here, in the order a picture travels through the rail:
 *
 * 1. {@link useAttachmentDraft} — what a composer has chosen but not yet posted.
 *    Nothing has been sent anywhere while these are held, so removing one
 *    invokes nothing and cancelling the composer discards the lot (CMT-FR-46).
 * 2. {@link AttachControl} and {@link PendingAttachmentStrip} — the affordance
 *    that adds one and the strip that shows what is queued (CMT-FR-45).
 * 3. {@link CommentAttachmentList} — what a posted comment renders, an image as
 *    a thumbnail and anything else as a named chip (CMT-FR-48).
 *
 * A stored attachment's bytes are fetched only when the card holding them is on
 * screen (CMT-FR-49), which is why the fetch is a hook on the leaf rather than a
 * preload on the rail: a review carrying dozens of screenshots costs nothing for
 * the threads the author is not looking at.
 */
import { useEffect, useRef, useState } from "react";
import { readCommentAttachment } from "../api";
import { Icon } from "./icons";
import {
  openReview,
  undecidedHunks,
  useProposalReference,
} from "../state/draftProposals";
import { setFocusedHunk } from "../state/draftDiscussion";
import {
  openPromptReviewFor,
  usePromptProposalReference,
} from "../state/promptProposals";
import {
  attachmentName,
  isImageAttachment,
  isProposalAttachment,
  undecidedCount,
  type Attachment,
  type AttachmentInput,
} from "../types";

/**
 * The media type of a link, guessed from its path.
 *
 * A guess rather than a probe: resolving the address to ask what it serves would
 * be a network request, and CMS-FR-43 records a link without ever fetching it.
 * An extension we do not recognise yields a type the backend refuses
 * (CMS-FR-47), which the card reports — better than inventing `image/png` for a
 * link that turns out to be something else entirely.
 */
export function guessMediaType(pathOrName: string): string {
  const ext = pathOrName.split("?")[0].split("#")[0].split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    case "svg":
      return "image/svg+xml";
    case "bmp":
      return "image/bmp";
    case "avif":
      return "image/avif";
    case "pdf":
      return "application/pdf";
    case "txt":
    case "log":
    case "md":
      return "text/plain";
    default:
      return "application/octet-stream";
  }
}

/**
 * The schemes an attachment's address may be rendered into an `href` or a `src`.
 *
 * The backend refuses anything else at write time (CMS-FR-43), so this is the
 * second line rather than the first — but the first line guards *writes*, and
 * what reaches here is a line out of a log file that is committed to a
 * repository, shared through a merge, and editable by hand. A `javascript:` URL
 * in an `href` is script execution, so the render path checks for itself rather
 * than trusting a value that arrived from disk.
 */
const RENDERABLE_SCHEMES = ["http:", "https:"];

/** The address to render, or `null` when it is not one we will link to. */
export function safeUrl(raw: string): string | null {
  try {
    return RENDERABLE_SCHEMES.includes(new URL(raw).protocol) ? raw : null;
  } catch {
    // Not an absolute URL at all — a relative one has no origin here, and a
    // malformed one is not something to hand to the browser.
    return null;
  }
}

/**
 * A `data:` URL for stored bytes, built only for a media type we are willing to
 * inline.
 *
 * Deliberately narrower than the set the backend accepts: `application/pdf` and
 * `text/plain` are served as a download chip rather than inlined, and anything
 * a crafted line might claim — `text/html` above all — yields nothing at all,
 * because a `data:text/html` URL in an `href` is a page that runs script.
 */
export function inlineDataUrl(mediaType: string, base64: string): string | null {
  const base = mediaType.split(";")[0].trim().toLowerCase();
  // The shape is checked, not just the prefix: `image/png,text/html` starts
  // with `image/` and would otherwise be pasted into the URL verbatim, where
  // the comma ends the type and `text/html` is what the browser would honour.
  // A media type carries exactly one `/` and no separators of its own.
  if (!/^[a-z0-9][a-z0-9!#$&^_.+-]*\/[a-z0-9][a-z0-9!#$&^_.+-]*$/.test(base)) {
    return null;
  }
  // SVG is excluded even though the backend accepts it as an image: an SVG
  // document can carry script, and the thumbnail below is wrapped in an anchor
  // to its own source, so inlining one would put a scriptable document behind a
  // click. It renders as a named chip instead — the picture is lost, the hole
  // is not. A *hosted* SVG is unaffected: that path renders the http(s) address
  // `safeUrl` already vetted, not bytes this application inlined.
  if (base === "image/svg+xml") return null;
  return base.startsWith("image/") ? `data:${base};base64,${base64}` : null;
}

/**
 * A `data:` URL for bytes we will not inline — a PDF, a text file, an SVG.
 *
 * Always `application/octet-stream`, whatever the line claims the type is, so
 * activating the chip saves the file instead of asking the browser to render a
 * document of a type an attacker chose. That is what lets CMT-FR-48's "opens it"
 * hold for a non-image without reintroducing the hole `inlineDataUrl` closes.
 */
export function downloadDataUrl(base64: string): string {
  return `data:application/octet-stream;base64,${base64}`;
}

/** Base64 of a file's bytes, with no `data:` prefix — what CMS-FR-43 expects. */
export function encodeBytes(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

/** Turn a picked, pasted, or dropped file into the input an append carries. */
export async function fileToInput(file: File): Promise<AttachmentInput> {
  const buffer = await file.arrayBuffer();
  return {
    kind: "inline",
    // A browser leaves `type` empty for an extension it does not know; falling
    // back to the name keeps a legitimate `.png` from being refused for it.
    mediaType: file.type !== "" ? file.type : guessMediaType(file.name),
    filename: file.name !== "" ? file.name : "attachment",
    data: encodeBytes(new Uint8Array(buffer)),
  };
}

/** A pending attachment, with the label the strip shows for it. */
export interface PendingAttachment {
  input: AttachmentInput;
  name: string;
}

export interface AttachmentDraft {
  pending: PendingAttachment[];
  addFiles: (files: readonly File[]) => Promise<void>;
  addLink: (url: string, label: string) => void;
  removeAt: (index: number) => void;
  clear: () => void;
  /** What a post carries; empty is the ordinary case (CMT-FR-47). */
  inputs: AttachmentInput[];
}

/**
 * CMT-FR-46: what a composer has queued, held until the comment is posted.
 *
 * `controlled` hands the strip to an owner outside this component. A conversation
 * the author has moved out of its rail keeps its pending attachments through
 * every mode change and through the surface it was in being unmounted
 * (`CVP-conversation-presentation.md` CVP-FR-47), which local state cannot do —
 * so its presentation instance holds them and passes them in here. Every composer
 * of a conversation nobody has moved passes nothing and keeps its own state,
 * which is what makes the registry cost such a conversation nothing.
 */
export function useAttachmentDraft(controlled?: {
  pending: readonly PendingAttachment[];
  onChange: (next: PendingAttachment[]) => void;
}): AttachmentDraft {
  const [local, setLocal] = useState<PendingAttachment[]>([]);
  const pending = controlled ? [...controlled.pending] : local;
  // Read through a ref so two files dropped in one gesture do not each start
  // from the same stale list, as `useDiscussionComposer` does for the same reason.
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  const setPending = (
    update: (prev: PendingAttachment[]) => PendingAttachment[],
  ) => {
    if (controlled) controlled.onChange(update(pendingRef.current));
    else setLocal(update);
  };

  const addFiles = async (files: readonly File[]) => {
    const encoded = await Promise.all(
      files.map(async (file) => ({
        input: await fileToInput(file),
        name: file.name !== "" ? file.name : "attachment",
      })),
    );
    setPending((prev) => [...prev, ...encoded]);
  };

  const addLink = (url: string, label: string) => {
    const trimmed = url.trim();
    if (trimmed === "") return;
    const name = label.trim();
    setPending((prev) => [
      ...prev,
      {
        input: {
          kind: "url",
          url: trimmed,
          mediaType: guessMediaType(trimmed),
          ...(name === "" ? {} : { label: name }),
        },
        name: name === "" ? trimmed : name,
      },
    ]);
  };

  return {
    pending,
    addFiles,
    addLink,
    removeAt: (index) => setPending((prev) => prev.filter((_, i) => i !== index)),
    clear: () => setPending(() => []),
    inputs: pending.map((p) => p.input),
  };
}

/**
 * CMT-FR-45: the Attach control, offering a file and a link.
 *
 * Two entries rather than one because the protocol carries both kinds and each
 * is reached differently — a file comes from the machine, a link is typed. Drop
 * and paste are wired by the composer that owns this control rather than here,
 * since they are events on the text area, not on the button.
 */
export function AttachControl({
  disabled,
  onFiles,
  onLink,
  onOpen,
  dismissSignal,
  align = "trailing",
}: {
  disabled: boolean;
  onFiles: (files: readonly File[]) => void;
  onLink: (url: string, label: string) => void;
  /** CMT-FR-32: told when this surface opens, so the card can close its others. */
  onOpen?: () => void;
  /** CMT-FR-32: raised by the card when one of its *other* surfaces opened. */
  dismissSignal?: number;
  /**
   * Which of the control's own edges the menu hangs from — the edge that has
   * the composer behind it, so the menu opens across the field rather than out
   * of it. A control leading its field (`.comment-composer__row`) anchors
   * leading; one trailing a control row anchors trailing, the default.
   */
  align?: "leading" | "trailing";
}) {
  const [open, setOpen] = useState(false);
  const [linking, setLinking] = useState(false);
  const [url, setUrl] = useState("");
  const [label, setLabel] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  /**
   * Close, and put focus back where it came from.
   *
   * Without the second half, dismissing the menu drops focus to `<body>` and a
   * keyboard user's next Tab restarts from the top of the document rather than
   * from the composer they were working in.
   */
  const close = (restoreFocus: boolean) => {
    setOpen(false);
    setLinking(false);
    if (restoreFocus) toggleRef.current?.focus();
  };

  // CMT-FR-32: the card's other transient surfaces dismiss this one.
  useEffect(() => {
    if (dismissSignal === undefined) return;
    setOpen(false);
    setLinking(false);
  }, [dismissSignal]);

  // CMT-FR-32: Escape or an outside pointer-down closes it, and neither invokes
  // anything. `mousedown` rather than `click` so the surface is gone before
  // whatever was clicked acts — the same treatment the card's overflow menu has.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      close(true);
    };
    const onDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) close(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [open]);

  const confirmLink = () => {
    if (url.trim() === "") return;
    onLink(url, label);
    setUrl("");
    setLabel("");
    close(true);
  };

  return (
    <div
      className="comment-attach"
      data-align={align}
      /* CVP-FR-44: a surface that takes Escape for itself. The detached overlay
         minimizes on Escape, and its React handler runs at the root container —
         above this control but below the `document` listener that closes this
         menu — so nothing this control does to the event can stop it. The
         overlay reads this flag instead, and dismissing a menu never costs the
         author the conversation it was opened in. */
      data-surface-open={open ? "true" : undefined}
      ref={rootRef}
    >
      <button
        ref={toggleRef}
        className="btn btn--ghost btn--icon-sm"
        aria-label="Attach"
        title="Attach"
        aria-expanded={open}
        disabled={disabled}
        data-testid="comment-attach-button"
        onClick={() => {
          const next = !open;
          setOpen(next);
          setLinking(false);
          if (next) onOpen?.();
        }}
      >
        <Icon.Paperclip size={13} />
      </button>
      <input
        ref={fileRef}
        type="file"
        multiple
        hidden
        aria-hidden="true"
        tabIndex={-1}
        data-testid="comment-attach-file-input"
        onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          // Cleared so choosing the same file twice in a row still fires.
          e.target.value = "";
          if (files.length > 0) onFiles(files);
          close(false);
        }}
      />
      {open && !linking && (
        <div className="comment-attach__menu" role="menu">
          <button role="menuitem" onClick={() => fileRef.current?.click()}>
            Attach a file…
          </button>
          <button role="menuitem" onClick={() => setLinking(true)}>
            Attach a link…
          </button>
        </div>
      )}
      {open && linking && (
        <div className="comment-attach__menu comment-attach__link" role="group">
          <input
            className="input"
            aria-label="Attachment address"
            placeholder="Address of the image or file"
            value={url}
            autoFocus
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                confirmLink();
              }
            }}
          />
          <input
            className="input"
            aria-label="Attachment label"
            placeholder="Label, optional"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
          <button
            className="btn btn--ghost"
            aria-label="Attach link"
            disabled={url.trim() === ""}
            onClick={confirmLink}
          >
            Attach link
          </button>
        </div>
      )}
    </div>
  );
}

/** CMT-FR-46: what is queued, each entry removable before the comment is posted. */
export function PendingAttachmentStrip({
  pending,
  onRemove,
}: {
  pending: readonly PendingAttachment[];
  onRemove: (index: number) => void;
}) {
  if (pending.length === 0) return null;
  return (
    <div className="comment-attach__pending" data-testid="comment-pending-attachments">
      {pending.map((item, i) => (
        <span key={i} className="comment-attach__chip">
          {item.input.kind === "url" ? (
            <Icon.Link size={11} />
          ) : (
            <Icon.Paperclip size={11} />
          )}
          <span className="comment-attach__chip-name">{item.name}</span>
          <button
            aria-label={`Remove attachment ${item.name}`}
            title="Remove attachment"
            onClick={() => onRemove(i)}
          >
            <Icon.X size={11} />
          </button>
        </span>
      ))}
    </div>
  );
}

/**
 * CMT-FR-49: a stored attachment's bytes, fetched when the card renders it.
 *
 * `null` while loading and `"error"` when the read was refused — the two are
 * distinguished so the chip can say it could not be loaded rather than sitting
 * blank forever (CMT-FR-49).
 */
type BlobSource =
  /** Still being read. */
  | { state: "loading" }
  /** Read, and safe to inline — a thumbnail's `src`. */
  | { state: "inline"; url: string }
  /** Read, but not something to render — a chip that saves the file. */
  | { state: "download"; url: string }
  /** Not read, and not worth reading until the reader asks for it. */
  | { state: "deferred" }
  /** The read was refused, so there is nothing to show or to save. */
  | { state: "error" };

function useBlobSource(
  threadId: string,
  draftId: string | undefined,
  attachment: Attachment,
): readonly [BlobSource, () => Promise<string | null>] {
  // Only a thumbnail needs the bytes. A chip renders a filename and a media
  // type, both of which the log line already carries, so reading the file would
  // decode a whole PDF into memory to draw two words — and would turn a refused
  // read into "could not be loaded" for content the card was never going to
  // show. `wanted` is what keeps the read to what is actually rendered.
  const wanted = attachment.kind === "blob" && isImageAttachment(attachment);
  const [src, setSrc] = useState<BlobSource>(
    wanted ? { state: "loading" } : { state: "deferred" },
  );
  const digest = attachment.kind === "blob" ? attachment.digest : "";

  useEffect(() => {
    if (!wanted) return;
    let cancelled = false;
    void readCommentAttachment({ discussionId: threadId, digest, draftId })
      .then((content) => {
        if (cancelled) return;
        // "We will not inline this" is not "this failed": a PDF is a perfectly
        // good attachment that renders as a named chip (CMT-FR-48) and saves on
        // activation, rather than one that could not be loaded (CMT-FR-49).
        const inline = inlineDataUrl(content.mediaType, content.data);
        setSrc(
          inline !== null
            ? { state: "inline", url: inline }
            : { state: "download", url: downloadDataUrl(content.data) },
        );
      })
      .catch(() => {
        if (!cancelled) setSrc({ state: "error" });
      });
    return () => {
      cancelled = true;
    };
  }, [threadId, draftId, digest, wanted]);

  /**
   * Fetch a deferred attachment because the reader activated its chip.
   *
   * The bytes were not worth reading to draw a filename; they are worth reading
   * the moment someone asks to open the thing (CMT-FR-48). Returns the URL to
   * open, or null when the read was refused — at which point the chip flips to
   * saying so, exactly as a thumbnail's failed read does.
   */
  const load = async (): Promise<string | null> => {
    try {
      const content = await readCommentAttachment({ discussionId: threadId, digest, draftId });
      const url =
        inlineDataUrl(content.mediaType, content.data) ??
        downloadDataUrl(content.data);
      setSrc({ state: "download", url });
      return url;
    } catch {
      setSrc({ state: "error" });
      return null;
    }
  };

  return [src, load] as const;
}

/**
 * CMT-FR-48: a proposal reference renders as a **control**, not a preview.
 *
 * It holds no content to show, and what it names is a decision the author has to
 * make rather than a picture they have to see — so it is the one attachment
 * whose rendering is an action. CMT-FR-49: it fetches nothing, so a conversation
 * carrying a proposal costs the rail exactly what one carrying a sentence costs.
 *
 * CMT-FR-67: the state comes from `DCP-draft-change-proposals.md` by way of the
 * store rather than from the attachment, which carries the identity alone — so a
 * control shows whatever the proposal has since become.
 *
 * CTA-FR-UKIG: and says nothing about it until a reading has answered. A control
 * the reading does not hold is **inert** — the path alone, no word about the
 * proposal, opening nothing — and states that the change is no longer available
 * only once a reading that completed did not return it, on the same terms an
 * attachment whose content could not be loaded says so. The difference is the
 * author's only route to the decision: a control that disowns a proposal still
 * standing leaves it undecidable, with the draft's one pending slot held and the
 * agent that made it refused another (DCP-FR-04).
 */
function ProposalAttachmentView({
  attachment,
}: {
  attachment: Extract<Attachment, { kind: "proposal" }>;
}) {
  // CTA-FR-LOOE: asking for the reading it needs is part of rendering it.
  const reference = useProposalReference(
    attachment.draftId,
    attachment.proposalId,
  );
  const state = reference.kind === "known" ? reference.proposal.state : null;
  // CMT-FR-48: the two controls are rendered alike, so they say the same words.
  const label = proposalControlLabel(state, reference.kind === "gone");

  const proposal = reference.kind === "known" ? reference.proposal : null;
  const counts = proposal?.counts;
  const changes = proposal?.hunkCount ?? 0;
  const left = undecidedCount(counts);

  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-proposal"
      data-state={state ?? (reference.kind === "gone" ? "missing" : "unresolved")}
      disabled={state === null}
      title={label === null ? attachment.path : `${label} — ${attachment.path}`}
      onClick={() => {
        // DCR-FR-HVXK / DCR-FR-03: the route to the review is a scroll of the
        // document column to the first change still to decide, rather than
        // something opening over the draft.
        if (proposal) {
          const first = undecidedHunks(proposal)[0];
          if (first) setFocusedHunk(proposal.draftId, first.id);
        }
        openReview(attachment.proposalId);
      }}
    >
      <Icon.Diff size={11} />
      <span className="comment-attach__chip-name">{attachment.path}</span>
      {/* DCR-FR-HVXK: how much the proposal changes, so the author knows what
          they are being asked to read before they go and read it. */}
      {changes > 0 && (
        <span className="comment-attach__proposal-count">
          {changes} {changes === 1 ? "change" : "changes"}
        </span>
      )}
      {/* CTA-FR-UKIG: an unresolved control carries no word about the proposal —
          the type span is absent rather than empty, so nothing on it can be
          read as a claim about what the proposal has become. */}
      {label !== null && (
        <span className="comment-attach__chip-type">{label}</span>
      )}
      {/* DCR-FR-HVXK: where the review is. The document column is to the left of
          the conversation this is read in, and saying so is what makes the
          control's own words explain what activating it will do. */}
      {left > 0 && (
        <span className="comment-attach__proposal-route">Review on the left</span>
      )}
    </button>
  );
}

/**
 * CMT-FR-48: a `prompt_proposal` reference renders as a **control** on exactly
 * the terms its `proposal` sibling does — rendered alike, naming its file alike,
 * and activated alike. Which review it opens is decided by the attachment's own
 * kind and by nothing else: this one names a prompt artifact of the project and
 * opens the review of `PCR-prompt-change-review.md` (PCR-FR-03), which opens or
 * focuses that artifact's Editor tab first where it is not already open
 * (PCR-FR-18).
 *
 * CMT-FR-67: the state comes from `PCP-prompt-change-proposals.md` by way of the
 * store rather than from the attachment, which carries the identity alone.
 * CTA-FR-SACG: and says nothing about it until a reading has answered — a control
 * that disowned a proposal still standing would leave it undecidable, with the
 * artifact's one pending slot held and the agent that made it refused another
 * (PPC-FR-09).
 */
function PromptProposalAttachmentView({
  attachment,
}: {
  attachment: Extract<Attachment, { kind: "promptProposal" }>;
}) {
  // CTA-FR-LOOE: asking for the reading it needs is part of rendering it.
  const reference = usePromptProposalReference(
    attachment.artifactId,
    attachment.proposalId,
  );
  const state = reference.kind === "known" ? reference.proposal.state : null;
  const label = proposalControlLabel(state, reference.kind === "gone");

  return (
    <button
      type="button"
      className="comment-attach__proposal"
      data-testid="comment-attachment-prompt-proposal"
      data-state={state ?? (reference.kind === "gone" ? "missing" : "unresolved")}
      disabled={state === null}
      title={label === null ? attachment.path : `${label} — ${attachment.path}`}
      // PCR-FR-18: the tab is opened or focused first where it is not already
      // open, so the review is never rendered anywhere but over the tab for its
      // own file.
      onClick={() =>
        openPromptReviewFor(attachment.artifactId, attachment.proposalId)
      }
    >
      <Icon.Diff size={11} />
      <span className="comment-attach__chip-name">{attachment.path}</span>
      {label !== null && (
        <span className="comment-attach__chip-type">{label}</span>
      )}
    </button>
  );
}

/**
 * CMT-FR-48 / CMT-FR-67 / CTA-FR-UKIG: what a proposal control says, for both
 * reference kinds.
 *
 * One function so the two are rendered alike: the rail keeps no second treatment
 * for either, and a sentence that drifted between them would be the difference
 * an author reads as a difference in what the control does.
 */
function proposalControlLabel(
  state: "pending" | "accepted" | "rejected" | null,
  gone: boolean,
): string | null {
  if (state === "accepted") return "Change accepted";
  if (state === "rejected") return "Change rejected";
  if (state === "pending") return "Review proposed change";
  // CTA-FR-SACG: the statement, in the register CMT-FR-49's chip says "could not
  // be loaded" in. A sentence would not fit beside the path within the rail's
  // measure, and one cut at "This proposed change is no l…" states nothing.
  return gone ? "No longer available" : null;
}

function AttachmentView({
  threadId,
  draftId,
  attachment,
}: {
  threadId: string;
  draftId?: string;
  attachment: Attachment;
}) {
  const name = attachmentName(attachment);
  const [stored, loadStored] = useBlobSource(threadId, draftId, attachment);
  const [broken, setBroken] = useState(false);

  /**
   * What this attachment resolves to, whichever kind it is.
   *
   * A link is checked rather than trusted (see `safeUrl`) — it arrived from a
   * log file — and, having no bytes here, it is never "loading". A blob comes
   * back through the backend as a URL this module built (CMT-FR-49).
   */
  const source: BlobSource =
    attachment.kind === "url"
      ? ((url) =>
          url === null
            ? ({ state: "error" } as const)
            : isImageAttachment(attachment)
              ? ({ state: "inline", url } as const)
              : ({ state: "download", url } as const))(safeUrl(attachment.url))
      : stored;

  // An image whose bytes the browser then refused is as broken as one that
  // never arrived, so `broken` collapses into the same state.
  const failed = source.state === "error" || broken;

  // CMT-FR-48: an image renders as a thumbnail bounded to the card's measure,
  // and activating it opens it at full size beyond the card.
  if (source.state === "inline" && !failed) {
    return (
      <a
        className="comment-attach__thumb"
        href={source.url}
        target="_blank"
        rel="noreferrer"
        data-testid="comment-attachment-image"
        title={name}
      >
        <img src={source.url} alt={name} onError={() => setBroken(true)} />
      </a>
    );
  }

  // Its bytes are still on the way. Named already, so the card does not jump
  // when they land.
  if (source.state === "loading") {
    return (
      <span className="comment-attach__chip" data-testid="comment-attachment-loading">
        <Icon.Paperclip size={11} />
        <span className="comment-attach__chip-name">{name}</span>
      </span>
    );
  }

  // CMT-FR-48: anything else is a named chip carrying its filename and media
  // type. CMT-FR-49: one whose content could not be loaded says so and names
  // what it points at, rather than leaving an empty frame on the card.
  //
  // A deferred chip has no `href` because it has no bytes yet — activating it
  // reads them and then opens, which is the whole point of not having read them
  // to draw a filename.
  const deferred = source.state === "deferred";
  return (
    <a
      className="comment-attach__chip"
      data-testid={failed ? "comment-attachment-broken" : "comment-attachment-chip"}
      data-broken={failed || undefined}
      href={failed || deferred ? undefined : source.url}
      // Focusable while deferred: without an `href` an anchor leaves the tab
      // order, and CMT-FR-35's keyboard reachability applies here too.
      role={deferred ? "button" : undefined}
      tabIndex={deferred ? 0 : undefined}
      target="_blank"
      rel="noreferrer"
      title={failed ? `${name} could not be loaded` : name}
      onClick={
        deferred
          ? (e) => {
              e.preventDefault();
              void loadStored().then((url) => {
                if (url !== null) window.open(url, "_blank", "noreferrer");
              });
            }
          : undefined
      }
      onKeyDown={
        deferred
          ? (e) => {
              if (e.key !== "Enter" && e.key !== " ") return;
              e.preventDefault();
              void loadStored().then((url) => {
                if (url !== null) window.open(url, "_blank", "noreferrer");
              });
            }
          : undefined
      }
    >
      {attachment.kind === "url" ? <Icon.Link size={11} /> : <Icon.Paperclip size={11} />}
      <span className="comment-attach__chip-name">{name}</span>
      <span className="comment-attach__chip-type">
        {failed
          ? "could not be loaded"
          : isProposalAttachment(attachment)
            ? attachment.path
            : attachment.mediaType}
      </span>
    </a>
  );
}

/**
 * CMT-FR-48 / CMT-FR-50: a posted comment's attachments, below its body.
 *
 * Read-only by construction: there is no removal control anywhere here, because
 * an attachment is immutable with the comment that carries it.
 */
export function CommentAttachmentList({
  threadId,
  draftId,
  attachments,
}: {
  threadId: string;
  /** CMS-FR-49: set for a thread in a draft, so the read is scoped to it. */
  draftId?: string;
  attachments: readonly Attachment[];
}) {
  if (attachments.length === 0) return null;
  return (
    <div className="comment__attachments" data-testid="comment-attachments">
      {attachments.map((a, i) =>
        a.kind === "proposal" ? (
          <ProposalAttachmentView key={i} attachment={a} />
        ) : a.kind === "promptProposal" ? (
          <PromptProposalAttachmentView key={i} attachment={a} />
        ) : (
          <AttachmentView
            key={i}
            threadId={threadId}
            draftId={draftId}
            attachment={a}
          />
        ),
      )}
    </div>
  );
}
