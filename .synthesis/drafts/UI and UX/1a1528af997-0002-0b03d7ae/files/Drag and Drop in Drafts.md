## Change Request

Drafts panel should be extended with 2 additional features to improve UX.

### Change 1:

The status filter row of the Drafts panel gets a fifth button, **Published**, after **All**. The five buttons stay one radio group (Active, Archived, Grad, All, Published), with a short tag and the full name on hover, as the other four have.

- Once the author selects **Published**, the tree shows only drafts whose publication store holds at least one record (the marker of DRP-FR-TQUB), on any status (active, archived, graduated, legacy `published`). The legacy `published` status alone does not decide it.
- The position narrows drafts the way the status filter does today, and it AND-combines with the text filter. Folders stay visible (DRP-FR-07, DRP-FR-29).
- The position is persisted per worktree like the other status positions (DRP-FR-14): `status_filter` accepts `published` in `save_drafts_panel_state` and `load_drafts_panel_state`. The default stays `active`.
- The row keeps clipping at its trailing edge at narrow widths, and the leading, trailing, and active buttons each render (SNV-FR-62, SNV-FR-63).

Specifications this change touches: `specifications/ui/DRP-drafts-panel.md` (filter wireframe and layout notes that say “four buttons”, DRP-FR-07, DRP-FR-14), `specifications/core/PSS-project-settings-storage.md` (PSS-FR-20, `status_filter` values and persisted-state text), and the draft “Published but active” (its Drafts panel filter rule, which must list the new position).

### Change 2:

Drag-and-Drop is specified in DRP-FR-27 (draft rows and folder rows onto a folder row or the implicit root) but does not work today. Make it work as DRP-FR-27 states it, with no new behaviour. Valid targets are highlighted, refused targets are never highlighted, and a move commits only on release over a valid target (`"move draft to folder"`, `"move drafts folder"`).

- First find the cause of the failure and fix that. Check in particular whether the Tauri window setting `dragDropEnabled` (default on) lets the webview receive HTML5 drag events on every supported platform.
- The fix must not break the image drop in the New Artifact tab (NAW) or the planned-chip drag in the specification map.
- The keyboard route (**Move to Folder…**, DRP-FR-28) stays as it is.

## Requirements

- Look-and-Feel of the appication must stay the same as today. 
- Drag-and-Drop should be limited to the Drafts panel only. No other panel gets a new drag source or drop target.
- The fifth button reuses the existing button treatment. No new colours, controls, or layout.