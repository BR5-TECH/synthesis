/**
 * Which reveal requests a vertical panel has already acted on
 * (LIB-FR-18, DRP-FR-34, CHG-FR-54, and SNV-FR-68's one-shot rule).
 *
 * A reveal request is a **standing prop** carrying a nonce, not an event: the
 * shell sets it and leaves it there. Each panel therefore has to remember which
 * nonce it has acted on, or it would re-apply the same reveal on a later render.
 *
 * That memory cannot live in the panel, which is the whole reason this module
 * exists. `VPanel` unmounts a panel outright whenever the vertical panel shows
 * another surface, so a component-local ref is lost the moment the author
 * switches panels — and coming back would re-apply a long-finished reveal over
 * the selection and filters they have since set, which is exactly what
 * SNV-FR-68 forbids ("they may … switch to another panel entirely, and nothing
 * … restores the tab's own item over what they did").
 *
 * Nonces are minted from one monotonic counter in the shell, so a single
 * high-water mark is enough for every panel: a request is spent once its nonce
 * is at or below the mark. It is reset with the viewport (OVW-FR-11 /
 * OVW-FR-12), where the counter's own continuity stops mattering because every
 * pending request is dropped with it.
 */

/** The highest nonce any panel has finished acting on. */
let consumed = 0;

/** Whether this request has already been acted on and must not run again. */
export function revealAlreadyConsumed(nonce: number): boolean {
  return nonce <= consumed;
}

/**
 * Record that a request has been acted on — whether that meant revealing the
 * item or deciding it cannot be resolved (SNV-FR-67). Both are terminal: a
 * request the panel has ruled on must not be reconsidered by a later reload,
 * which is what keeps a refresh from selecting a substitute.
 */
export function markRevealConsumed(nonce: number): void {
  if (nonce > consumed) consumed = nonce;
}

/**
 * Drop every record. Called when the viewport resets — a project switch or an
 * active-worktree change — and by tests.
 */
export function resetPanelReveals(): void {
  consumed = 0;
}
