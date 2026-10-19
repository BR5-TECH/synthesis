## Intent

Today, once the Draft is published to GitHub it’s moved to graduated.

That was a bad decision, and needs to be changed since it’s often misleading.

Publishing the draft should not affect its state. An active draft stays active. A graduated draft stays graduated. (An archived draft still cannot be published until it is restored, per GHP-FR-GJEO.)

Users: authors who publish drafts to GitHub and read the Drafts panel.

## User journey

- The author publishes or updates a draft on GitHub from the action control.
- The draft keeps its status. The author sees the draft still under the same Drafts panel filter as before.
- The draft row shows a ⤴ marker. Its hover text reads `Published · 4h ago` (the current publication record's relative time). The marker is also a link to the issue, as today.

## Requirements

1. **Publication never changes status.** A successful publication or update appends its history record to the publication store and writes nothing to the draft's status, whether it is `active` or `graduated`. Remove `set_draft_published` and its only caller. No operation sets `published` any more.
2. `published` **becomes a legacy value.** A draft already stored with status `published` is not migrated on load. It keeps reading as `published` (label `Published`, as today) until the next write to the draft: save, rename, move, or archive. That write stores `active` (an archive stores `archived`, not `active`). A publication alone is not such a write. A legacy `published` draft stays editable, renamable, and graduatable as today.
3. **Publication fact.** Whether a draft was published comes from its publication store (DRS-FR-EJBM), on any status, and not from its status.
4. **Drafts panel filters.** **Grad** admits `graduated` only. A graduated draft is one that a graduation run inside the IDE published. A legacy `published` draft shows under **Active** and **All**, and not under **Grad**. **Published** shows only the drafts whose publication store holds at least one record, on any status (active, archived, graduated, legacy `published`). The legacy `published` status alone does not decide it.
5. **Drafts panel row.** A row whose publication store holds a record shows the ⤴ marker, on every status (active, archived, graduated, legacy `published`). The marker's hover text and accessible name read `Published · <relative time>`. The row's second line does not change: it keeps today's text for graduated, run-held, and plain-time rows. The row never states `Published · 4h ago` on its second line. A legacy `published` row keeps its `Published` status label and also shows the marker.
6. **Other surfaces.** The draft tab's publication tag, **Update on GitHub**, and the Information modal's publication group do not change. They already read the publication store.
7. **Publish eligibility is unchanged.** `active`, legacy `published`, and `graduated` drafts can be published. `archived`, GitHub-shadow, and run-locked drafts are refused as today.

## Specifications this work changes

- `specifications/core/DRS-draft-storage.md`: DRS-FR-VKQO (no setter; `published` is a legacy read value that the next write replaces), DRS-FR-OGZC (store and status stay separate facts).
- `specifications/core/GHP-github-publication.md`: GHP-FR-BSYH (publication sets no status), GHP-FR-GJEO (`published` is legacy), GHP-FR-VNCS wording if needed.
- `specifications/ui/NAW-new-artifact.md`: NAW-FR-16 (status positions; `published` is no longer set by publication).
- `specifications/ui/DRP-drafts-panel.md`: **Grad** filter text and wireframe note, the `Published · 4h ago` row wireframe (now the ⤴ marker with hover text only), and the matching requirements on filters and row content.