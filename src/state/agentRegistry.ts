/**
 * A counter bumped whenever the agent registry or a project's enrolment
 * changes, so every surface showing them re-reads.
 *
 * The three surfaces of `AGT-agents.md` are peers with no shared parent below
 * `App`: the chrome roster lives in the top chrome, the Agents section in a
 * Global settings window, the enrolment section in a Project settings window — and
 * two of them are routinely on screen at once. Threading a callback between
 * them would put agent plumbing in every layer in between, so the invalidation
 * travels as a module-level store instead, on the same `useSyncExternalStore`
 * pattern `diffModes` uses.
 *
 * Mostly a *counter* rather than the records themselves: the settings section
 * reads the whole registry while the chrome roster reads the open project's
 * enrolment, so for those two there is nothing one could hand the other. They
 * only need to be told to ask again.
 *
 * The one exception is the **enrolment itself**, which is published here by the
 * chrome control that already reads it (AGT-FR-02) and read by surfaces that
 * must not read it for themselves. The Comments panel is why: AGT-FR-29 has a
 * tag render bold wherever a message body is rendered, the panel included, but
 * `CMP-comments-panel.md` gives that panel exactly one list call and says it
 * invokes nothing else (CMP-FR-18). Sharing the read the chrome has already made
 * is what satisfies both — the panel gets the roster and issues no call for it.
 *
 * This is not a subscription to the store file. Another process editing
 * `synthesis.toml` bumps nothing — a surface learns of that on its next read.
 */
import { useSyncExternalStore } from "react";
import type { ProjectAgent } from "../types";

let revision = 0;
const subscribers = new Set<() => void>();

/**
 * Called after a write lands — an agent created, edited, or deleted, or one
 * enrolled in or withdrawn from a project. Not called for a *failed* write,
 * which changed nothing for anyone to re-read.
 */
export function notifyAgentRegistryChanged(): void {
  revision += 1;
  for (const notify of subscribers) notify();
}

export function subscribe(notify: () => void): () => void {
  subscribers.add(notify);
  return () => {
    subscribers.delete(notify);
  };
}

function snapshot(): number {
  return revision;
}

/** The current revision; changes when an agent write lands. */
export function useAgentRegistryRevision(): number {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

// ---------------------------------------------------------------------------
// The published enrolment
// ---------------------------------------------------------------------------

/**
 * The open project's enrolled agents as the last read saw them.
 *
 * A module-level snapshot rather than a fetch, because every surface reading it
 * is one the specs give no call of its own. Empty until the chrome control's read
 * lands, which renders a tag as prose for that moment and corrects itself when it
 * does — losing emphasis briefly is a far better failure than a panel that
 * bolds a name the project does not enrol.
 */
let projectAgents: readonly ProjectAgent[] = [];

/**
 * Published by the surface that read the enrolment (AGT-FR-02), so its peers do
 * not have to read it again.
 *
 * Replaced wholesale rather than merged: the read it comes from is the authority
 * on who is enrolled, and an agent withdrawn from the project has to actually
 * leave. The identity is compared first so a re-read returning the same roster
 * notifies nobody — this is called on every registry revision, and waking every
 * subscriber for an unchanged list would re-render them for nothing.
 */
export function publishProjectAgents(list: readonly ProjectAgent[]): void {
  if (sameRoster(projectAgents, list)) return;
  projectAgents = list;
  for (const notify of subscribers) notify();
}

/** Whether two reads describe the same enrolment, agent for agent. */
function sameRoster(
  a: readonly ProjectAgent[],
  b: readonly ProjectAgent[],
): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  return a.every(
    (entry, i) =>
      entry.agent.id === b[i].agent.id &&
      entry.agent.nickname === b[i].agent.nickname &&
      entry.availability === b[i].availability,
  );
}

/**
 * The published enrolment, as it stands.
 *
 * The same reference until `publishProjectAgents` replaces it, which is what
 * `useSyncExternalStore` requires of a snapshot — a fresh array per call is an
 * infinite re-render loop rather than a cosmetic inefficiency.
 */
export function readProjectAgents(): readonly ProjectAgent[] {
  return projectAgents;
}

/**
 * The open project's enrolled agents, for a surface that must not read them
 * itself. Re-renders when a read publishes a different roster.
 */
export function useProjectAgents(): readonly ProjectAgent[] {
  return useSyncExternalStore(subscribe, readProjectAgents, readProjectAgents);
}

/** Test seam: forget every write, so one test's revision is not served to the next. */
export function resetAgentRegistry(): void {
  revision = 0;
  projectAgents = [];
  subscribers.clear();
}
