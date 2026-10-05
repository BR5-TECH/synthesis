/**
 * Which owners of a discussion still resolve in the active project
 * (`CVP-conversation-presentation.md` CVP-FR-45).
 *
 * The shell publishes the artifacts it knows. A discussion about an artifact
 * that is not in the set has an unavailable owner. Before the first
 * publication, every owner counts as available, because an empty answer that
 * was never read must not mark every conversation as orphaned.
 *
 * Session memory only. A project or worktree change resets it.
 */
import { useSyncExternalStore } from "react";

import type { DiscussionTarget } from "../types";

let artifactIds: ReadonlySet<string> | null = null;
let draftNames: ReadonlyMap<string, string> = new Map();
let noteLabels: ReadonlyMap<string, string> = new Map();
let version = 0;
const listeners = new Set<() => void>();

function emit(): void {
  version += 1;
  listeners.forEach((l) => l());
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Publish the artifact ids and paths the project holds now. */
export function publishKnownArtifacts(ids: Iterable<string>): void {
  const next = new Set(ids);
  if (
    artifactIds &&
    artifactIds.size === next.size &&
    [...next].every((id) => artifactIds?.has(id))
  ) {
    return;
  }
  artifactIds = next;
  emit();
}

/** Publish the names of the drafts the project holds now. */
export function publishKnownDrafts(
  drafts: readonly { id: string; name: string }[],
): void {
  const next = new Map(drafts.map((d) => [d.id, d.name]));
  if (
    next.size === draftNames.size &&
    [...next].every(([id, name]) => draftNames.get(id) === name)
  ) {
    return;
  }
  draftNames = next;
  emit();
}

/** Remember how a note was labelled when its conversation was last revealed. */
export function rememberNoteLabel(noteId: string, label: string): void {
  if (noteLabels.get(noteId) === label) return;
  noteLabels = new Map(noteLabels).set(noteId, label);
  emit();
}

/** Forget what was published, as a project or worktree change does. */
export function resetOwnerAvailability(): void {
  if (artifactIds === null && draftNames.size === 0 && noteLabels.size === 0) return;
  artifactIds = null;
  draftNames = new Map();
  noteLabels = new Map();
  emit();
}

/**
 * The name an owner is called by in a row, a tab name, and an announcement: the
 * project-relative path of an artifact, the name of a draft, or the label of a
 * note.
 */
export function ownerLabelOf(target: DiscussionTarget): string {
  switch (target.kind) {
    case "artifact":
      return target.artifactId;
    case "draft":
      return draftNames.get(target.draftId) ?? "Draft";
    case "note":
      return noteLabels.get(target.noteId) ?? "Note";
  }
}

/** Whether the thing a discussion is about still resolves. */
export function isOwnerAvailable(target: DiscussionTarget): boolean {
  if (target.kind !== "artifact") return true;
  if (artifactIds === null) return true;
  return artifactIds.has(target.artifactId);
}

/** Re-renders its caller when any published owner fact changes. */
export function useOwnerFacts(): number {
  return useSyncExternalStore(
    subscribe,
    () => version,
    () => version,
  );
}

/** {@link isOwnerAvailable}, followed in React. */
export function useOwnerAvailable(target: DiscussionTarget | null | undefined): boolean {
  useOwnerFacts();
  return target ? isOwnerAvailable(target) : true;
}
