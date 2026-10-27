/**
 * The rail's **whole-target opening composer** (`CMT-comments.md` CMT-FR-QHTW,
 * `ACT-action-control.md` ACT-FR-BRZK).
 *
 * On an Editor tab that lends the rail a margin, **Discuss** opens this composer
 * at the head of the Discussion section instead of a floating one. It is the
 * shared `DiscussionComposer` in opening mode, about the whole item and never a
 * fragment. It is one of the rail's transient surfaces (CMT-FR-32): Escape and a
 * press outside it close it. A post in flight holds it until it lands, so a
 * refusal renders where it was refused (ACT-FR-17).
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { DiscussionComposer } from "../discussion";
import type { OpenDiscussionRequest } from "../discussion";
import { isDiagramEditorOpen } from "../DiagramEditor/store";
import type { Discussion, DiscussionTarget, ProjectAgent } from "../../types";

export interface WholeTargetOpening {
  target: DiscussionTarget;
  /** What the item is called in the placeholder: "file" or "specification". */
  noun: string;
  /** A counter: each Discuss puts the caret in the field again. */
  focusSignal: number;
  onOpen: (request: OpenDiscussionRequest) => Promise<Discussion>;
  onOpened: (discussion: Discussion) => void;
  onClose: () => void;
}

interface WholeTargetComposerProps {
  opening: WholeTargetOpening;
  agents: readonly ProjectAgent[];
  disabled: boolean;
  blocked: boolean;
}

export function WholeTargetComposer({
  opening,
  agents,
  disabled,
  blocked,
}: WholeTargetComposerProps) {
  const { onOpen, onClose } = opening;
  const ref = useRef<HTMLDivElement | null>(null);
  const [posting, setPosting] = useState(false);

  const open = useCallback(
    async (request: OpenDiscussionRequest) => {
      setPosting(true);
      try {
        return await onOpen(request);
      } finally {
        setPosting(false);
      }
    },
    [onOpen],
  );

  // ACT-FR-13 / CMT-FR-32: a disabled field cannot take focus, so the surface
  // itself takes it, with its stated reason in the rail above. Escape then
  // reaches it on the same terms as an enabled composer.
  const inert = disabled || blocked;
  useEffect(() => {
    if (inert) ref.current?.focus();
  }, [inert, opening.focusSignal]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (isDiagramEditorOpen()) return;
      // AGT-FR-30: one Escape closes the mention picker and keeps the composer.
      if (document.querySelector('[data-testid="mention-picker"]')) return;
      if (!ref.current?.contains(document.activeElement)) return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    const onDown = (e: MouseEvent) => {
      if (isDiagramEditorOpen() || posting) return;
      const el = e.target as Node | null;
      if (el && ref.current?.contains(el)) return;
      onClose();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onDown);
    };
  }, [onClose, posting]);

  return (
    <div
      ref={ref}
      className="comment-rail__opening"
      tabIndex={-1}
      role="group"
      aria-label={`Discuss this ${opening.noun}`}
      data-testid="whole-target-composer"
    >
      <DiscussionComposer
        mode="opening"
        target={opening.target}
        fragmentTarget={null}
        agents={agents}
        disabled={disabled}
        blocked={blocked}
        ariaLabel={`Discuss this ${opening.noun}`}
        placeholder={`What should be done with this ${opening.noun}?`}
        focusSignal={opening.focusSignal}
        onOpen={open}
        onOpened={opening.onOpened}
      />
    </div>
  );
}
