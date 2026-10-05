# Font enumeration

**Spec code:** `FNT`

## Intent
Backend facility that reports which font families the machine has available for rendering text, so the Appearance section of `../ui/GLS-global-settings.md` can offer the author the fonts they actually have rather than a list someone guessed at. It exists because a typographic choice the user cannot see the result of is not a choice: an author who has installed the coding face they read best expects to find it in the picker, and a curated list would never contain it. The report is a read of what the operating system already knows, so it costs nothing beyond the query and reflects the machine as it stands at the moment of the call. Out of scope: this module installs, downloads, validates, previews, and caches nothing across launches, and it holds no opinion about which family a role should use — it reports availability, and every choice, constraint, and fallback belongs to the surface that presents the controls.

## Contract surface

### Tauri commands
- `"list system fonts"` → `list_system_fonts()` → `[FontFamily]` — the font families available to the application for rendering text at the moment of the call, ordered per FNT-FR-03. Returns an empty list when none can be enumerated, and never returns an error.

### Payload shapes
```
FontFamily {
  family,       // the family name as the platform registers it, e.g. "JetBrains Mono"
  monospace     // true when the family's faces advance every glyph by a fixed width
}
```

## Functional requirements
1. **FNT-FR-01** `"list system fonts"` exists as a Tauri command with the documented payload shape. In the walking-skeleton build the implementation may return a fixed synthetic list; the Appearance section of `../ui/GLS-global-settings.md` must be fully exercisable against it.
2. **FNT-FR-02** The command returns one entry per font family available to the application, deduplicated by family name: a family carrying several faces — weights, italics, widths — appears exactly once, because a typographic role selects a family rather than a face.
3. **FNT-FR-03** Entries are ordered by family name under a case-insensitive collation, so a consumer renders them in the order returned without sorting again.
4. **FNT-FR-04** `monospace` reports whether the family's faces advance every glyph by a fixed width. It is a description of the family and not a permission: it never restricts which role a family may be chosen for, and a consumer is free to present a proportional family for a code role (per `../ui/GLS-global-settings.md` GLS-FR-18).
5. **FNT-FR-05** The command reads only what the operating system already knows about the fonts installed on this machine. It installs nothing, downloads nothing, reads no project file, and changes no state anywhere in the application or on disk.
6. **FNT-FR-06** The result reflects the machine at the moment of the call: a font installed or removed while the application is running is reflected by the next call, with no relaunch required.
7. **FNT-FR-07** The command never fails. A platform whose font set cannot be read yields an empty list, which a consumer reads as "no family beyond the application's built-in faces is offerable" rather than as an error to surface.
8. **FNT-FR-08** The application's own built-in default faces are not entries in this list. They need no enumeration because they are always available, and the Appearance section offers them independently of whatever this command returns (per `../ui/GLS-global-settings.md` GLS-FR-18).
9. **FNT-FR-09** This module persists nothing and holds no state across calls. The families chosen for the three typographic roles live in the app-preferences record (`GSS-global-settings-storage.md` GSS-FR-29), which this module neither reads nor writes; it likewise never validates a stored family, so a family absent from its result is resolved by `../ui/GLS-global-settings.md` GLS-FR-22.
10. **FNT-FR-10** The command answers identically whether or not a project is open and whichever worktree is active, because the fonts a machine has installed are a property of the machine rather than of any project.

## Non-functional requirements
- Enumeration requires no network access and no filesystem access outside whatever the platform's own font APIs perform.
- Enumeration completes fast enough on a machine carrying a large font set to populate the Appearance controls without an intermediate loading state; a consumer may hold the result for the lifetime of the surface rather than re-querying it per interaction.
- The command allocates in proportion to the number of installed families and retains nothing after returning.
