// Shared builders for test records of the unified discussion model.

import type {
  Comment,
  Discussion,
  DiscussionListItem,
  DiscussionTarget,
  FragmentRange,
  FragmentTarget,
} from "../types";

/**
 * Fields a test may give besides the record itself. They stand for the
 * owner-and-range shape of a discussion in the short form tests like to write.
 */
export type DiscussionOverrides = Partial<Discussion> & {
  id: string;
  /** Short for an artifact owner with this path. */
  artifactId?: string;
  /** Short for a fragment target on the owner's path. `null` is whole-target. */
  anchor?: FragmentRange | null;
};

/**
 * Build a discussion. By default it is a fragment discussion on `path`, with one
 * comment given by the caller or an empty list.
 */
export function buildDiscussion(
  over: DiscussionOverrides,
  defaults: { path: string; range: FragmentRange; comments?: Comment[] },
): Discussion {
  const { artifactId, anchor, ...rest } = over;
  const path = artifactId ?? defaults.path;
  const target: DiscussionTarget = rest.target ?? { kind: "artifact", artifactId: path };
  const range = anchor === undefined ? defaults.range : anchor;
  const fragmentTarget =
    rest.fragmentTarget !== undefined
      ? rest.fragmentTarget
      : range === null
        ? null
        : { owner: target, path, ...range };
  return {
    comments: defaults.comments ?? [],
    locked: false,
    resolved: false,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
    ...rest,
    target,
    fragmentTarget,
  };
}

/** Build a list item of the project-wide listing. */
export function buildListItem(
  discussion: Discussion,
  ownerUnavailable = false,
): DiscussionListItem {
  return { discussion, ownerUnavailable };
}

/** A fragment target on an artifact path (default `a.md`) for the given range. */
export function fragment(range: FragmentRange, path = "a.md"): FragmentTarget {
  return { owner: { kind: "artifact", artifactId: path }, path, ...range };
}
