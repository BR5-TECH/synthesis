/**
 * The activity bar's toggle semantics, shared by both clusters
 * (`SNV-shell-navigation.md` SNV-FR-45 for the vertical panel, SNV-FR-46 for
 * the bottom panel).
 *
 * Both clusters behave identically, which is the point of putting the rule in
 * one place: a toggle is not a radio button. Activating the surface the panel
 * is already rendering closes that panel; activating any other surface opens
 * the panel on it. The two panels differ only in which state they hold, so this
 * works on the shape they have in common.
 */

/** A panel's visibility plus which surface it is bound to. */
export interface PanelToggleState<S> {
  /** True when the panel renders nothing and only the activity bar is visible. */
  hidden: boolean;
  /** The surface the panel is bound to, whether or not it is currently shown. */
  surface: S;
}

/**
 * Resolve one click on the toggle for `clicked`.
 *
 * The selected surface is deliberately *preserved* when the panel is hidden
 * (SNV-FR-45): it is what the panel returns to, and the persisted width
 * fraction is left alone alongside it, so re-activating the same toggle
 * restores exactly what was there. Reopening is not modelled as "restore the
 * remembered surface", though — activating a *different* toggle while hidden
 * opens the panel on that surface instead, which is why the hidden branch takes
 * `clicked` rather than `current.surface`.
 */
export function nextPanelToggleState<S>(
  current: PanelToggleState<S>,
  clicked: S,
): PanelToggleState<S> {
  if (current.hidden) return { hidden: false, surface: clicked };
  if (current.surface === clicked) {
    return { hidden: true, surface: current.surface };
  }
  return { hidden: false, surface: clicked };
}

/**
 * Whether a cluster's toggle for `surface` reads as the active one.
 *
 * A hidden panel marks none of its toggles active (SNV-FR-45 / SNV-FR-46) —
 * the strip says where the panel *is*, and a hidden panel is nowhere.
 */
export function isToggleActive<S>(
  state: PanelToggleState<S>,
  surface: S,
): boolean {
  return !state.hidden && state.surface === surface;
}
