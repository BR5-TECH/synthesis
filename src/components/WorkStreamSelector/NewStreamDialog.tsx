/**
 * Creating a work stream
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-XZRO,
 * WSS-FR-CRJD).
 *
 * Two inputs and no more: a name, and the branch the stream is created from.
 * A refusal renders inline against the field it is about and the dialog stays
 * open with what the author typed.
 */

import { useEffect, useState } from "react";

import * as api from "../../api";
import { STREAM_ERRORS, type BranchEntry } from "../../types";
import { refusalText, splitRefusal } from "./refusals";

export function NewStreamDialog({
  activeBranch,
  onClose,
  onCreated,
}: {
  activeBranch: string;
  onClose: () => void;
  onCreated: () => void;
}) {
  const [name, setName] = useState("");
  const [branch, setBranch] = useState(activeBranch);
  const [branches, setBranches] = useState<BranchEntry[]>([]);
  const [nameError, setNameError] = useState<string | null>(null);
  const [branchError, setBranchError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    api
      .listWorktreesAndBranches()
      .then((listing) => setBranches(listing.branches))
      .catch(() => setBranches([]));
  }, []);

  const submit = () => {
    setNameError(null);
    setBranchError(null);
    if (name.trim() === "") {
      setNameError("A stream needs a name.");
      return;
    }
    setRunning(true);
    api
      .createWorkStream(name.trim(), branch || undefined)
      .then(() => {
        setRunning(false);
        onCreated();
      })
      .catch((reason) => {
        setRunning(false);
        // WSS-FR-CRJD: rendered against the field it is about, and the dialog
        // stays open with what the author typed still in it.
        const [code] = splitRefusal(String(reason));
        if (
          code === STREAM_ERRORS.nameTaken ||
          code === STREAM_ERRORS.nameInvalid
        ) {
          setNameError(refusalText(String(reason)));
          return;
        }
        if (code === STREAM_ERRORS.baseBranchRequired) {
          setBranchError(refusalText(String(reason)));
          return;
        }
        setNameError(refusalText(String(reason)));
      });
  };

  return (
    <div className="scrim" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-labelledby="new-stream-title">
        <div className="modal__head">
          <h2 className="modal__title" id="new-stream-title">
            New work stream
          </h2>
        </div>
        <div className="modal__body">
          <label className="picker-field">
            <span className="picker-field__label">Name</span>
            <input
              className="input"
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
            />
          </label>
          {nameError && (
            <p className="stream-select__error" role="alert">
              {nameError}
            </p>
          )}
          <label className="picker-field">
            <span className="picker-field__label">Created from</span>
            <select
              className="input"
              value={branch}
              onChange={(e) => setBranch(e.target.value)}
            >
              {branches.length === 0 && <option value={branch}>{branch}</option>}
              {branches.map((entry) => (
                <option key={entry.name} value={entry.name}>
                  {entry.name}
                </option>
              ))}
            </select>
          </label>
          {branchError && (
            <p className="stream-select__error" role="alert">
              {branchError}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button
            className="btn btn--ghost btn--sm"
            disabled={running}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            className="btn btn--primary btn--sm"
            disabled={running}
            onClick={submit}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
