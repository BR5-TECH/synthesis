/**
 * The in-place rename of a draft in the chrome of its tab (`NAW-new-artifact.md`
 * NAW-FR-04, NAW-FR-25). Split out of `index.tsx` to keep that file within the
 * size rule.
 */
import { useEffect, useState } from "react";

import * as api from "../../api";
import type { DraftRecord } from "../../types";
import type { DraftSessionStore } from "../../state/draftSessions";

export interface DraftRename {
  renaming: boolean;
  nameText: string;
  nameError: string | null;
  start: () => void;
  edit: (text: string) => void;
  cancel: () => void;
  commit: () => Promise<void>;
}

export function useDraftRename(input: {
  draftId: string;
  name: string;
  drafts: DraftSessionStore;
  record: DraftRecord | null;
  dirty: boolean;
  saveCurrent: () => Promise<boolean>;
  applyRecord: (next: DraftRecord) => void;
  loadHistory: () => Promise<void>;
  onNameChanged: (name: string) => void;
  onDraftChanged: () => void;
}): DraftRename {
  const {
    draftId,
    name,
    drafts,
    record,
    dirty,
    saveCurrent,
    applyRecord,
    loadHistory,
    onNameChanged,
    onDraftChanged,
  } = input;
  const [renaming, setRenaming] = useState(false);
  const [nameText, setNameText] = useState("");
  /**
   * NAW-FR-04: why the name the author typed could not be taken. Rendered
   * against the field it was typed into rather than in the tab's error banner,
   * and the field stays open with the text in it, because the correction is one
   * character away and a banner would send the author back to reopen the field.
   */
  const [nameError, setNameError] = useState<string | null>(null);

  // The rename field opens seeded with the name currently in force, whichever
  // surface last set it.
  useEffect(() => setNameText(name), [name]);

  /** Back out of the chrome's rename, restoring the name in force. */
  const cancel = () => {
    setRenaming(false);
    setNameError(null);
    setNameText(name);
  };

  const commit = async () => {
    const next = nameText.trim();
    if (!next || next === name) {
      setRenaming(false);
      setNameError(null);
      setNameText(name);
      return;
    }
    // NAW-FR-25: renaming the draft renames the prompt, so a dirty buffer is
    // written under its current name first. A timer firing after the rename
    // would write through the old path and recreate the file just renamed away.
    const oldPrompt = record?.promptPath ?? null;
    if (oldPrompt !== null && dirty && !(await saveCurrent())) return;
    try {
      const updated = await api.renameDraft(draftId, next);
      setRenaming(false);
      setNameError(null);
      const newPrompt = updated.promptPath ?? null;
      // NAW-FR-25: the edit session under the old path is carried across so the
      // rename costs no buffer and no undo history. Applied before the record,
      // so the selection is never briefly a path nothing holds.
      if (oldPrompt !== null && newPrompt !== null && newPrompt !== oldPrompt) {
        drafts.renamePath(draftId, oldPrompt, newPrompt);
      }
      applyRecord(updated);
      onNameChanged(updated.name);
      onDraftChanged();
      // The versions already in the rail keep the path each was taken at
      // (DHS-FR-06); the live prompt's path is what moved.
      void loadHistory();
    } catch (e) {
      // Refused against this field, which stays open — nothing moved, on disk
      // or in the record.
      setNameError(String(e));
    }
  };

  return {
    renaming,
    nameText,
    nameError,
    start: () => setRenaming(true),
    edit: (text: string) => {
      setNameText(text);
      setNameError(null);
    },
    cancel,
    commit,
  };
}
