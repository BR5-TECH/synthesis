/**
 * The project's classified artifacts, as the Flow canvas's artifact-association
 * picker offers them (`FLO-flow.md` FLO-FR-10) and as it resolves a node's
 * stored artifact references to display names and types for rendering
 * (FLO-FR-11).
 *
 * Fed by `"load project tree (filesystem nodes tagged by artifact type)"`, the
 * operation FLO delegates for exactly this. The canvas is remounted per Flow
 * tab, so the list is loaded here rather than threaded down from the shell — and
 * refreshed on `"project tree changed"`, because an artifact created or deleted
 * while a Flow tab is open changes what resolves (FLO-FR-11).
 */
import { useEffect, useMemo, useState } from "react";

import * as api from "../api";
import { onProjectTreeChanged } from "../events";
import type { ArtifactType, TreeNode } from "../types";

/** One artifact a node may reference. */
export interface ArtifactOption {
  /** The stable, path-derived key stored on the node (ASC-FR-13). */
  id: string;
  /** Basename, what the picker and the node fall back to. */
  name: string;
  /** Project-relative path, what the picker lists so two basenames are told apart. */
  path: string;
  artifactType: ArtifactType;
  /**
   * FLO-FR-11: what the picker and the node render. The basename for every
   * artifact whose filename identifies it, and the name declared in the file
   * for a skill — whose file is always `SKILL.md`, so its filename identifies
   * nothing. The scan resolves which is which (ASC-FR-19); nothing here reads
   * an artifact's contents to find out.
   */
  displayName: string;
}

export interface ProjectArtifacts {
  /** Every classified artifact, which is what a stored reference resolves against. */
  artifacts: ArtifactOption[];
  /** FLO-FR-10: the subset the picker offers. */
  referenceable: ArtifactOption[];
  /**
   * Whether a tree has actually been read. Load-bearing for FLO-FR-11: before
   * one has, EVERY stored reference would compare as unresolved, so the canvas
   * must not render the warning row yet — a Flow opening in a project whose
   * scan is still in flight would otherwise flash "unresolved" across every node.
   */
  loaded: boolean;
}

/**
 * FLO-FR-10: the artifact types a Flow node can be built out of. A Flow wires
 * prompts and the instructions around them into a workflow, so those are what
 * the picker offers; a spec, a scenario, a scratchpad, an agent definition or
 * another Flow is not a step in one, and listing them buries the artifacts that
 * are behind the ones that never will be.
 */
const REFERENCEABLE_TYPES: ReadonlySet<ArtifactType> = new Set<ArtifactType>([
  "skill",
  "prompt",
  "instructions",
]);

/**
 * A skill is its `SKILL.md`. The supporting files that live beside it in the
 * skill's folder — references, templates, scripts — classify as `skill` too
 * because they sit under a skills directory, but none of them is a skill a node
 * can reference, so only the entry point is offered.
 */
function isSkillEntryPoint(a: ArtifactOption): boolean {
  return a.name.toLowerCase() === "skill.md";
}

/** FLO-FR-10: the artifacts the picker offers, out of everything classified. */
export function referenceableArtifacts(
  artifacts: ArtifactOption[],
): ArtifactOption[] {
  return artifacts.filter(
    (a) =>
      REFERENCEABLE_TYPES.has(a.artifactType) &&
      (a.artifactType !== "skill" || isSkillEntryPoint(a)),
  );
}

/** Every classified file in the tree, depth-first in tree order. */
export function flattenArtifacts(root: TreeNode): ArtifactOption[] {
  const out: ArtifactOption[] = [];
  const walk = (node: TreeNode) => {
    for (const c of node.children ?? []) {
      if (c.nodeKind === "folder") walk(c);
      else if (c.artifactType) {
        out.push({
          id: c.id,
          name: c.name,
          path: c.path,
          artifactType: c.artifactType,
          displayName: c.displayName ?? c.name,
        });
      }
    }
  };
  walk(root);
  return out;
}

export function useProjectArtifacts(): ProjectArtifacts {
  const [artifacts, setArtifacts] = useState<ArtifactOption[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let latest = 0;
    let unlisten: (() => void) | undefined;

    const read = () => {
      // Which read this is. A structural change can arrive while an earlier read
      // is still in flight, and without this the older one's result can land
      // last and leave the picker offering a tree that no longer exists.
      const generation = ++latest;
      void api
        .loadProjectTree()
        .then((tree) => {
          if (cancelled || generation !== latest) return;
          setArtifacts(flattenArtifacts(tree));
          setLoaded(true);
        })
        // A tree that cannot be read leaves `loaded` false, so stored references
        // render as the ids they are rather than as warnings about artifacts
        // that may well still exist.
        .catch(() => {});
    };

    read();
    void onProjectTreeChanged(read).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const referenceable = useMemo(
    () => referenceableArtifacts(artifacts),
    [artifacts],
  );

  return { artifacts, referenceable, loaded };
}
