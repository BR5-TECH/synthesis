import type { ReactNode, SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

const I = ({
  children,
  size = 14,
  className = "icon",
  ...rest
}: IconProps & { children: ReactNode }) => (
  <svg
    className={className}
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill="none"
    stroke="currentColor"
    strokeWidth={1.6}
    strokeLinecap="round"
    strokeLinejoin="round"
    {...rest}
  >
    {children}
  </svg>
);

export const Icon = {
  Sigma: (p: IconProps) => (
    <I {...p}>
      <path d="M18 6H6l7 6-7 6h12" />
    </I>
  ),
  /**
   * CHG-FR-56: the rollback the Changes panel's footer performs — a counter-
   * clockwise arrow returning to where it came from, which is what the action
   * does to a file. Never the only indication of the action (CHG-FR-58).
   */
  Rollback: (p: IconProps) => (
    <I {...p}>
      <path d="M3 12a9 9 0 1 0 3-6.7" />
      <path d="M3 4v5h5" />
    </I>
  ),
  /** A failure a surface reports inline beside what it could not do. */
  Warning: (p: IconProps) => (
    <I {...p}>
      <path d="M12 3 2 20h20z" />
      <path d="M12 10v5" />
      <path d="M12 17.5v.5" />
    </I>
  ),
  Folder: (p: IconProps) => (
    <I {...p}>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </I>
  ),
  FolderOpen: (p: IconProps) => (
    <I {...p}>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H3z" />
      <path d="M3 9h18l-2 9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </I>
  ),
  File: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3v5h5" />
      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9z" />
    </I>
  ),
  Doc: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3v5h5" />
      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9z" />
      <path d="M8 13h8M8 17h5" />
    </I>
  ),
  Search: (p: IconProps) => (
    <I {...p}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3-3" />
    </I>
  ),
  Settings: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 0 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 0 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 0 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 0 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </I>
  ),
  Book: (p: IconProps) => (
    <I {...p}>
      <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
      <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
    </I>
  ),
  /** CMT-FR-29: the comments control in the Editor's action cluster. */
  Comment: (p: IconProps) => (
    <I {...p}>
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
    </I>
  ),
  /** CMT-FR-15: a locked thread's badge. */
  Lock: (p: IconProps) => (
    <I {...p}>
      <rect x="3" y="11" width="18" height="11" rx="2" />
      <path d="M7 11V7a5 5 0 0 1 10 0v4" />
    </I>
  ),
  Notes: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
      <path d="M14 3v6h6" />
      <path d="M8 13h6M8 17h4" />
    </I>
  ),
  Home: (p: IconProps) => (
    <I {...p}>
      <path d="m3 11 9-8 9 8" />
      <path d="M5 10v9a1 1 0 0 0 1 1h4v-6h4v6h4a1 1 0 0 0 1-1v-9" />
    </I>
  ),
  X: (p: IconProps) => (
    <I {...p}>
      <path d="M18 6 6 18M6 6l12 12" />
    </I>
  ),
  Plus: (p: IconProps) => (
    <I {...p}>
      <path d="M12 5v14M5 12h14" />
    </I>
  ),
  /**
   * NAW-FR-08: the New Artifact tab's file-rail toggle. Three bars rather than a
   * folder or a caret — it reveals a whole region of the tab, not a folder's
   * contents and not a collapsed list.
   */
  Menu: (p: IconProps) => (
    <I {...p}>
      <path d="M4 6h16M4 12h16M4 18h16" />
    </I>
  ),
  Caret: (p: IconProps) => (
    <I {...p}>
      <path d="m6 9 6 6 6-6" />
    </I>
  ),
  CaretUp: (p: IconProps) => (
    <I {...p}>
      <path d="m6 15 6-6 6 6" />
    </I>
  ),
  CaretRight: (p: IconProps) => (
    <I {...p}>
      <path d="m9 6 6 6-6 6" />
    </I>
  ),
  Play: (p: IconProps) => (
    <I {...p}>
      <path d="M6 4l14 8-14 8z" />
    </I>
  ),
  Stop: (p: IconProps) => (
    <I {...p}>
      <rect x="6" y="6" width="12" height="12" rx="1" />
    </I>
  ),
  Branch: (p: IconProps) => (
    <I {...p}>
      <circle cx="6" cy="6" r="2" />
      <circle cx="6" cy="18" r="2" />
      <circle cx="18" cy="8" r="2" />
      <path d="M6 8v8" />
      <path d="M18 10c0 6-6 4-6 8" />
    </I>
  ),
  /**
   * CHG-FR-01: the Changes panel — a file carrying an addition and a deletion.
   * Deliberately not `Branch`: that glyph belongs to the Git panel at the foot
   * of the strip (SNV-FR-44), and two toggles wearing it read as two Git
   * buttons.
   */
  Diff: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3v5h5" />
      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9z" />
      <path d="M12 10v4M10 12h4M10 17h4" />
    </I>
  ),
  Commit: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="3" />
      <path d="M3 12h6M15 12h6" />
    </I>
  ),
  GitPull: (p: IconProps) => (
    <I {...p}>
      <circle cx="6" cy="6" r="2" />
      <circle cx="6" cy="18" r="2" />
      <circle cx="18" cy="18" r="2" />
      <path d="M6 8v8" />
      <path d="M18 12V8a4 4 0 0 0-4-4h-2" />
      <path d="m9 7-3-3 3-3" />
    </I>
  ),
  History: (p: IconProps) => (
    <I {...p}>
      <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
      <path d="M3 3v5h5" />
      <path d="M12 7v5l3 2" />
    </I>
  ),
  /**
   * Circular arrows: redrawing what is already in hand. `data-icon` because
   * WTS-FR-37 is a requirement *about which glyph renders*, and a `<path d>`
   * is not something a test should have to read to check it.
   */
  Refresh: (p: IconProps) => (
    <I data-icon="refresh" {...p}>
      <path d="M21 12a9 9 0 1 1-3-6.7" />
      <path d="M21 3v5h-5" />
    </I>
  ),
  /**
   * WTS-FR-37: the top-chrome refresh control. Material arriving from
   * elsewhere — an arrow coming down into a tray — rather than the circular
   * arrows above. What the control does is reach across the network and bring
   * back what the remote holds, and an icon reading as "redraw this" invites
   * the author to press it when the chrome looks stale rather than when a
   * colleague has pushed.
   */
  PullDown: (p: IconProps) => (
    <I data-icon="pull-down" {...p}>
      <path d="M12 3v11" />
      <path d="m7 10 5 5 5-5" />
      <path d="M4 19h16" />
    </I>
  ),
  /**
   * GIT-FR-XXLE: the top-chrome Push control. Material leaving for elsewhere —
   * an arrow rising out of a tray — as the counterpart of the refresh control's
   * arrow coming down into one (WTS-FR-37).
   */
  PushUp: (p: IconProps) => (
    <I data-icon="push-up" {...p}>
      <path d="M12 15V4" />
      <path d="m7 8 5-5 5 5" />
      <path d="M4 19h16" />
    </I>
  ),
  Tag: (p: IconProps) => (
    <I {...p}>
      <path d="M3 12V5a2 2 0 0 1 2-2h7l9 9-9 9z" />
      <circle cx="7.5" cy="7.5" r="1.5" />
    </I>
  ),
  /**
   * `TAB-tabs.md` TAB-FR-35: the pinned marker a pinned tab carries, and
   * TAB-FR-33's leading icon on the menu's **Pin tab** entry.
   *
   * A drawing pin seen head-on with its shaft trailing down — the shape the
   * gesture is named after — so a marker the size of a few pixels still reads
   * as "pinned" rather than as a second dirty dot.
   */
  Pin: (p: IconProps) => (
    <I {...p}>
      <path d="M9 3h6l-1 6 3 3H7l3-3z" />
      <path d="M12 12v9" />
    </I>
  ),
  /** `TAB-tabs.md` TAB-FR-33: the same pin, struck through, for **Unpin tab**. */
  Unpin: (p: IconProps) => (
    <I {...p}>
      <path d="M9 3h6l-1 6 3 3H7l3-3z" />
      <path d="M12 12v9" />
      <path d="M4 4l16 16" />
    </I>
  ),
  /**
   * `TAB-tabs.md` TAB-FR-33: the leading icon on **Close tabs to the left** and
   * **Close tabs to the right**.
   *
   * An X with a bar on one side, the bar standing for the tab that survives the
   * close and the X for the tabs that do not. The strip mirrors it for the
   * leading direction, so one glyph serves both and the pair reads as two
   * halves of one action rather than as two unrelated ones.
   */
  CloseSide: (p: IconProps) => (
    <I {...p}>
      <path d="M4 4v16" />
      <path d="M10 8l8 8m0-8l-8 8" />
    </I>
  ),
  /**
   * `TAB-tabs.md` TAB-FR-33: the leading icon on **Close other tabs**.
   *
   * The same X between *two* bars, because the tab that survives this one has
   * tabs closing on both sides of it. Mirroring `CloseSide` would have drawn
   * the right-hand entry a second time, leaving two of the three mass closes
   * indistinguishable at a glance.
   */
  CloseOthers: (p: IconProps) => (
    <I {...p}>
      <path d="M3 4v16" />
      <path d="M21 4v16" />
      <path d="M8 8l8 8m0-8l-8 8" />
    </I>
  ),
  /**
   * `AGT-agents.md` AGT-FR-02: the top chrome's agents control, and the marker
   * on an agent-authored comment.
   *
   * A head in outline with an antenna — the conventional shorthand for "an AI
   * collaborator" — rather than the sparkle a generic "AI" affordance carries.
   * The distinction is the point: this chrome control names *who* the project
   * can talk to, and a sparkle would read as "do something clever here".
   */
  Agents: (p: IconProps) => (
    <I {...p}>
      <rect x="4" y="8" width="16" height="12" rx="3" />
      <path d="M12 4v4" />
      <circle cx="12" cy="3" r="1.4" />
      {/* Drawn as short segments rather than zero-length dots: a 1.6 stroke
          cap is under a pixel at the 14px this renders at, and antialiases to
          almost nothing. */}
      <path d="M9.2 13h.6M14.2 13h.6" />
      <path d="M9.5 16.5h5" />
    </I>
  ),
  Diamond: (p: IconProps) => (
    <I {...p}>
      <path d="M12 2 2 12l10 10 10-10z" />
    </I>
  ),
  /**
   * `FLO-flow.md` FLO-FR-07: what the Flow add menu offers. A step of the graph,
   * drawn as the box it renders as with the connection port on its trailing
   * edge that is what makes it a step rather than a card. Its companion in that
   * menu is `Refresh`, whose circular arrow is the mark a loop's own header
   * carries.
   */
  Node: (p: IconProps) => (
    <I {...p}>
      <rect x="3" y="6" width="14" height="12" rx="2" />
      <path d="M17 12h4" />
      <circle cx="21" cy="12" r="1.2" />
    </I>
  ),
  /* NAW-FR-17: publishing a draft's files up and out into the project. */
  Graduate: (p: IconProps) => (
    <I {...p}>
      <path d="M12 20V6" />
      <path d="m6 12 6-6 6 6" />
      <path d="M4 3h16" />
    </I>
  ),
  /*: the hand-off of a finished specification to an implementing
     agent — a flag planted on the thing that is ready to be built. */
  Flag: (p: IconProps) => (
    <I {...p}>
      <path d="M4 22V4a5 5 0 0 1 8 0 5 5 0 0 0 8 0v9a5 5 0 0 1-8 0 5 5 0 0 0-8 0" />
    </I>
  ),
  /**
   * GRU-FR-XQVG: the mark **Pause** carries in a rail row's action cluster —
   * the two bars every transport control pauses with.
   */
  Pause: (p: IconProps) => (
    <I {...p}>
      <rect x="6" y="5" width="4" height="14" rx="1" />
      <rect x="14" y="5" width="4" height="14" rx="1" />
    </I>
  ),
  /**
   * GRU-FR-XQVG: the mark **Resume** carries — the pause bars giving way to the
   * start triangle, so the pair reads as one act and its undoing.
   */
  Resume: (p: IconProps) => (
    <I {...p}>
      <path d="M7 5l11 7-11 7z" />
    </I>
  ),
  /**
   * GRU-FR-XQVG: the mark **Auto-start** carries when it is on — a run the
   * queue starts by itself.
   */
  AutoStartOn: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="M10 8.5l6 3.5-6 3.5z" />
    </I>
  ),
  /** GRU-FR-XQVG: the same mark struck through, for the value **off**. */
  AutoStartOff: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="9" />
      <path d="M10 8.5l6 3.5-6 3.5z" />
      <path d="M4 20 20 4" />
    </I>
  ),
  /**
   * GRU-FR-XQVG: the row's **dedicated drag handle** — its own control within
   * the row, separate from the selectable body and from every control of the
   * action cluster, so a drag never selects or archives a run.
   */
  DragHandle: (p: IconProps) => (
    <I {...p}>
      <circle cx="9" cy="6" r="1" />
      <circle cx="15" cy="6" r="1" />
      <circle cx="9" cy="12" r="1" />
      <circle cx="15" cy="12" r="1" />
      <circle cx="9" cy="18" r="1" />
      <circle cx="15" cy="18" r="1" />
    </I>
  ),
  /* NAW-FR-28: retiring a draft from the panel's view without taking anything
     off disk — a lidded box, not a wastebasket. */
  Archive: (p: IconProps) => (
    <I {...p}>
      <path d="M3 8h18v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />
      <path d="M2 3h20v5H2z" />
      <path d="M10 12h4" />
    </I>
  ),
  /**
   * GRH-FR-DYNU: the counterpart of `Archive` on a row already filed away — the
   * same box with the contents coming back out of it, so the pair reads as one
   * act and its undoing rather than as two unrelated marks.
   */
  Unarchive: (p: IconProps) => (
    <I {...p}>
      <path d="M3 8h18v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />
      <path d="M2 3h20v5H2z" />
      <path d="M12 18v-6" />
      <path d="m9 15 3-3 3 3" />
    </I>
  ),
  /**
   * GRU-FR-XQVG: the mark the icon-only **Discard run** control carries.
   *
   * A bin rather than an ✕ deliberately: the ✕ of `Icon.X` is the close control
   * every tab and every overlay of this window uses, and the one act this
   * section cannot undo must not be mistaken for the one that costs nothing.
   */
  /**
   * GRT-FR-AMHS: the mark **Restart** carries, in the run region and in the
   * rail row alike. Circular arrows around a start triangle: the run is begun
   * again from the same prompt, rather than the `Refresh` of redrawing what is
   * already in hand or the `Trash` of the act it stands in place of.
   */
  Restart: (p: IconProps) => (
    <I data-icon="restart" {...p}>
      <path d="M20 12a8 8 0 1 1-2.6-5.9" />
      <path d="M20 3v5h-5" />
      <path d="m10 9 5 3-5 3z" fill="currentColor" />
    </I>
  ),
  Trash: (p: IconProps) => (
    <I {...p}>
      <path d="M4 7h16" />
      <path d="M10 4h4a1 1 0 0 1 1 1v2H9V5a1 1 0 0 1 1-1z" />
      <path d="M6 7v13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V7" />
      <path d="M10 11v6M14 11v6" />
    </I>
  ),
  Live: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="3" fill="currentColor" />
      <circle cx="12" cy="12" r="8" />
    </I>
  ),
  /**
   * CTA-FR-MGVJ: the marker on a failed contribution. `data-icon` for the reason
   * `Refresh` carries one — that a card renders *this* glyph rather than the one
   * a delivered comment or a pending one carries is the requirement, and a
   * `<path d>` is not something a test should have to read to check it.
   */
  AlertTriangle: (p: IconProps) => (
    <I data-icon="alert-triangle" {...p}>
      <path d="M12 4 2.5 20h19z" />
      <path d="M12 10v4" />
      <path d="M12 17.5v.01" />
    </I>
  ),
  Check: (p: IconProps) => (
    <I {...p}>
      <path d="M4 12.5 9 17.5 20 6.5" />
    </I>
  ),
  /* CMT-FR-12: quoting a message inside a thread. */
  Quote: (p: IconProps) => (
    <I {...p}>
      <path d="M9 7H5a1 1 0 0 0-1 1v4a1 1 0 0 0 1 1h3v1a3 3 0 0 1-3 3" />
      <path d="M20 7h-4a1 1 0 0 0-1 1v4a1 1 0 0 0 1 1h3v1a3 3 0 0 1-3 3" />
    </I>
  ),
  /* CMT-FR-45: attaching a file to a comment, and the chip a stored one renders as. */
  Paperclip: (p: IconProps) => (
    <I {...p}>
      <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
    </I>
  ),
  /* CMT-FR-48: an attachment carried as an address rather than as content. */
  Link: (p: IconProps) => (
    <I {...p}>
      <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
      <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
    </I>
  ),
  /* Posting a composed message. */
  Send: (p: IconProps) => (
    <I {...p}>
      <path d="M21 3 10.5 13.5" />
      <path d="M21 3l-6.5 18-4-8-8-4z" />
    </I>
  ),
  Sun: (p: IconProps) => (
    <I {...p}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
    </I>
  ),
  Moon: (p: IconProps) => (
    <I {...p}>
      <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
    </I>
  ),
  Boxes: (p: IconProps) => (
    <I {...p}>
      <rect x="3" y="3" width="7" height="7" rx="1" />
      <rect x="14" y="3" width="7" height="7" rx="1" />
      <rect x="3" y="14" width="7" height="7" rx="1" />
      <rect x="14" y="14" width="7" height="7" rx="1" />
    </I>
  ),
  Bell: (p: IconProps) => (
    <I {...p}>
      <path d="M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9" />
      <path d="M10 21a2 2 0 0 0 4 0" />
    </I>
  ),
  Layers: (p: IconProps) => (
    <I {...p}>
      <path d="m12 2 10 6-10 6L2 8z" />
      <path d="m2 14 10 6 10-6" />
    </I>
  ),
  Terminal: (p: IconProps) => (
    <I {...p}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="m7 9 4 3-4 3M13 15h4" />
    </I>
  ),
  Code: (p: IconProps) => (
    <I {...p}>
      <path d="m8 8-4 4 4 4M16 8l4 4-4 4M13.5 6l-3 12" />
    </I>
  ),
  /* SMZ-FR-JWXA: zoom out — a centred bar, the pair of Plus. */
  Minus: (p: IconProps) => (
    <I {...p}>
      <path d="M5 12h14" />
    </I>
  ),
  Minimize: (p: IconProps) => (
    <I {...p}>
      <path d="M6 18h12" />
    </I>
  ),
  Attach: (p: IconProps) => (
    <I {...p}>
      <path d="M4 10h6V4" />
      <path d="M10 10 3 3" />
      <path d="M20 14h-6v6" />
      <path d="m14 14 7 7" />
    </I>
  ),
  /* FLO-FR-22: frame the whole graph — four corner brackets closing on a view. */
  Fit: (p: IconProps) => (
    <I {...p}>
      <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
    </I>
  ),
  /* DFV-FR-08: the three visualization modes. One column of stacked rows; two
     columns side by side; a single finished page. */
  Unified: (p: IconProps) => (
    <I {...p}>
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M4 9h16M4 14h16" />
    </I>
  ),
  SideBySide: (p: IconProps) => (
    <I {...p}>
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M12 4v16" />
    </I>
  ),
  Final: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3v5h5" />
      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9z" />
      <path d="m9 14 2 2 4-4" />
    </I>
  ),
  /**
   * SNV-FR-UCJH: the Documents toggle — a sheet with a second sheet behind it,
   * which says "reference files" and cannot be mistaken for the Project book or
   * for a single file.
   */
  Documents: (p: IconProps) => (
    <I {...p}>
      <path d="M8 7V4a1 1 0 0 1 1-1h8l4 4v10a1 1 0 0 1-1 1h-3" />
      <path d="M17 3v4h4" />
      <path d="M3 8a1 1 0 0 1 1-1h8l4 4v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />
      <path d="M7 14h5M7 17h3" />
    </I>
  ),
  /** DPN-FR-MICG: the format glyph of a PDF document row and of a PDF Viewer tab. */
  Pdf: (p: IconProps) => (
    <I {...p}>
      <path d="M14 3v5h5" />
      <path d="M19 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9z" />
      <path d="M7 17v-4h1.3a1.2 1.2 0 0 1 0 2.4H7M11.5 13v4M11.5 13h.9a2 2 0 0 1 0 4h-.9" />
    </I>
  ),
  /* DFV-FR-16: source text versus the rendered document. */
  Rich: (p: IconProps) => (
    <I {...p}>
      <path d="M6 5h12M12 5v14M9 19h6" />
    </I>
  ),
};
