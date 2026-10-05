//! Reading a log and folding its events into threads.

use super::*;

// ---------------------------------------------------------------------------
// Reading + folding (CMS-FR-06 … CMS-FR-10)
// ---------------------------------------------------------------------------

/// Every well-formed event in an artifact's log, in the order the lines appear.
///
/// CMS-FR-07 / CMS-FR-08: a line that is not valid JSON, carries an unrecognised
/// `type`, is missing a field its type requires, or carries a `v` this build does
/// not know is skipped rather than failing the read. One damaged line never costs
/// the author the rest of the conversation, and the line is left in the file
/// untouched.
pub(super) fn parse_events(text: &str) -> Vec<Event> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Event>(line).ok())
        .filter(|event| event.v == SCHEMA_VERSION)
        .collect()
}

pub(super) fn read_events(root: &fsa::RootFs, artifact_id: &str) -> Vec<Event> {
    let Ok(path) = log_path(root, artifact_id) else {
        return Vec::new();
    };
    let text = match root.read_text(&path) {
        Ok(text) => text,
        // No log yet is simply no threads; the first append creates it.
        Err(_) => return Vec::new(),
    };
    parse_events(&text)
}

/// CMS-FR-09: fold a log's events into the threads they describe.
///
/// Events are replayed in ascending `at` order, tie-broken by `event_id`, rather
/// than in the order the lines physically appear: two application processes
/// appending to one log interleave their lines by arrival, so physical order
/// carries no meaning (per `RMS-repository-machine-storage.md` RMS-FR-PFOB).
/// Sorting by the recorded instant is the only ordering both writers agree on.
/// (`at` is fixed-width UTC — see `notes::format_rfc3339_utc` — so a string
/// comparison is a chronological one.)
///
/// CMS-FR-06: the first event for a given `event_id` wins and every later one is
/// ignored, so a log a repeated append or an import pass duplicated a line in
/// folds exactly as the un-duplicated one does.
///
/// Which log is being folded, and therefore what the threads it yields are about.
///
/// The fold cannot read this off the events: a `thread_opened` line records where
/// the thread was *opened*, which a rename makes stale (CMS-FR-24), and neither
/// opening event records the scope at all. The caller knows which file it opened,
/// so it says so here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldTarget<'a> {
    /// A log under the store's `comments/` folder (CMS-FR-01).
    Artifact { artifact_id: &'a str },
    /// A draft's per-file log, keyed by the file's draft-relative path.
    DraftFile { draft_id: &'a str, file_rel: &'a str },
    /// CMS-FR-55: a draft's one reserved discussion log.
    DraftDiscussion { draft_id: &'a str },
    /// CMS-FR-55: an artifact's reserved discussion log.
    ArtifactDiscussion { artifact_id: &'a str },
    /// CMS-FR-55 / CMS-FR-62: a note's reserved discussion log, which holds the
    /// one discussion that note carries rather than every discussion of a
    /// target that may accumulate several.
    NoteDiscussion { note_id: &'a str },
}

impl<'a> FoldTarget<'a> {
    /// The owner every discussion of this log has, read off the log's location.
    pub fn owner(&self) -> DiscussionTarget {
        match *self {
            FoldTarget::Artifact { artifact_id } | FoldTarget::ArtifactDiscussion { artifact_id } => {
                DiscussionTarget::Artifact {
                    artifact_id: artifact_id.to_string(),
                }
            }
            FoldTarget::DraftFile { draft_id, .. } | FoldTarget::DraftDiscussion { draft_id } => {
                DiscussionTarget::Draft {
                    draft_id: draft_id.to_string(),
                }
            }
            FoldTarget::NoteDiscussion { note_id } => DiscussionTarget::Note {
                note_id: note_id.to_string(),
            },
        }
    }

    /// The path a fragment of this log carries, or `None` for a log that holds
    /// whole-target discussions.
    ///
    /// Read off the log's location rather than off the event, because an
    /// event's own path is where the discussion was opened, which a rename makes
    /// stale (CMS-FR-24).
    pub fn fragment_path(&self) -> Option<&'a str> {
        match *self {
            FoldTarget::Artifact { artifact_id } => Some(artifact_id),
            FoldTarget::DraftFile { file_rel, .. } => Some(file_rel),
            _ => None,
        }
    }
}

/// CMS-FR-15: a discussion whose events include no `comment_added` is dropped. A
/// torn or partially-merged log must not surface a discussion with nothing in it.
pub fn fold_events(artifact_id: &str, events: Vec<Event>) -> Vec<Discussion> {
    fold_events_for(FoldTarget::Artifact { artifact_id }, events)
}

/// The fragment a legacy or unified opening line stands for in this log.
fn fragment_in(
    target: FoldTarget<'_>,
    path: &str,
    start: usize,
    end: usize,
    quote: &str,
) -> FragmentTarget {
    FragmentTarget {
        owner: target.owner(),
        path: path.to_string(),
        start,
        end,
        quote: quote.to_string(),
    }
}

/// [`fold_events`] with the log's own identity made explicit (CMS-FR-36,
/// CMS-FR-53).
///
/// CMS-FR-04: this is where a legacy log is read as unified. A `thread_opened`
/// line is a `discussion_opened` whose fragment is the old anchor, a
/// `thread_reanchored` line is a `fragment_moved`, and a `discussion_opened` line
/// without a fragment is a whole-target discussion. The reading is a pure function
/// of the lines, so reading a log twice gives the same discussions and nothing is
/// ever rewritten.
pub fn fold_events_for(target: FoldTarget<'_>, events: Vec<Event>) -> Vec<Discussion> {
    let mut ordered: Vec<Event> = Vec::with_capacity(events.len());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for event in events {
        if seen.insert(event.event_id.clone()) {
            ordered.push(event);
        }
    }
    ordered.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.event_id.cmp(&b.event_id)));

    // Pass 1: every discussion that was opened.
    //
    // Deliberately separate from the pass that applies the rest. A single pass
    // would drop any event that sorted at or before its own opening event, and
    // that is not a hypothetical: `open_discussion_in` stamps the opening pair
    // with one `at`, `now_rfc3339` is second-granularity, so the pair is *always*
    // a tie and a single pass would rest on the `event_id` tie-break happening to
    // be monotone. A second application process appending into the same log has
    // no such guarantee, and the cost of getting it wrong is the comment being
    // dropped and the discussion vanishing entirely (CMS-FR-15 then filters the
    // comment-less discussion out). Opening is the only event that creates a
    // discussion, so collecting those first makes the fold genuinely
    // order-independent, as CMS-FR-09 promises.
    let mut order: Vec<String> = Vec::new();
    let mut discussions: HashMap<String, Discussion> = HashMap::new();
    let fragment_path = target.fragment_path();
    for event in &ordered {
        // An opening event of the *other* shape than the log holds is skipped
        // rather than folded: a whole-target opening in a fragment log, or a
        // fragment opening in a whole-target log, describes a discussion that log
        // cannot hold. Skipping keeps CMS-FR-56 true of a hand-edited or
        // mismerged log as well as of one this module wrote.
        let fragment: Option<Option<FragmentTarget>> = match (&event.body, fragment_path) {
            (EventBody::ThreadOpened { anchor, .. }, Some(path)) => Some(Some(fragment_in(
                target,
                path,
                anchor.start,
                anchor.end,
                &anchor.quote,
            ))),
            (
                EventBody::DiscussionOpened {
                    fragment_target: Some(f),
                    ..
                },
                Some(path),
            ) => Some(Some(fragment_in(target, path, f.start, f.end, &f.quote))),
            (
                EventBody::DiscussionOpened {
                    fragment_target: None,
                    ..
                },
                None,
            ) => Some(None),
            _ => None,
        };
        let Some(fragment_target) = fragment else {
            continue;
        };
        if discussions.contains_key(&event.thread_id) {
            // A second opening for one discussion id: the first is the one that
            // happened. Ignoring the later one keeps the fold total rather than
            // letting a crafted log rewrite a fragment.
            continue;
        }
        order.push(event.thread_id.clone());
        discussions.insert(
            event.thread_id.clone(),
            Discussion {
                id: event.thread_id.clone(),
                target: target.owner(),
                fragment_target,
                comments: Vec::new(),
                locked: false,
                resolved: false,
                created_at: event.at.clone(),
                updated_at: event.at.clone(),
            },
        );
    }

    // Pass 2: apply everything else, in the same order.
    for event in ordered {
        match event.body {
            EventBody::ThreadOpened { .. } | EventBody::DiscussionOpened { .. } => {}
            body => {
                // Every other event needs a discussion that was opened. One that
                // arrives for an unknown discussion is dropped rather than
                // conjuring a discussion with no owner.
                let Some(discussion) = discussions.get_mut(&event.thread_id) else {
                    continue;
                };
                match body {
                    EventBody::CommentAdded {
                        comment_id,
                        body,
                        quotes,
                        attachments,
                    } => {
                        if discussion.comments.iter().any(|c| c.id == comment_id) {
                            continue;
                        }
                        discussion.comments.push(Comment {
                            id: comment_id,
                            author: event.by,
                            body,
                            quotes,
                            attachments,
                            created_at: event.at.clone(),
                        });
                    }
                    EventBody::ThreadLocked => discussion.locked = true,
                    EventBody::ThreadUnlocked => discussion.locked = false,
                    EventBody::ThreadResolved => discussion.resolved = true,
                    EventBody::ThreadReopened => discussion.resolved = false,
                    // CMS-FR-59: a whole-target discussion has no fragment to
                    // follow, and the command that would append this refuses one.
                    // A log is hand-editable, so the fold stays total by ignoring
                    // a move that names a discussion without a fragment.
                    EventBody::ThreadReanchored { anchor } => {
                        if let Some(fragment) = discussion.fragment_target.as_mut() {
                            fragment.start = anchor.start;
                            fragment.end = anchor.end;
                            fragment.quote = anchor.quote;
                        }
                    }
                    EventBody::FragmentMoved { fragment_target } => {
                        if let Some(fragment) = discussion.fragment_target.as_mut() {
                            fragment.start = fragment_target.start;
                            fragment.end = fragment_target.end;
                            fragment.quote = fragment_target.quote;
                        }
                    }
                    EventBody::ThreadOpened { .. } | EventBody::DiscussionOpened { .. } => {
                        unreachable!("handled in pass 1")
                    }
                }
                // CMS-FR-10: `updated_at` is the instant of the latest event
                // folded in. Never rewritten, because no event is.
                if event.at > discussion.updated_at {
                    discussion.updated_at = event.at;
                }
            }
        }
    }

    let mut out: Vec<Discussion> = order
        .into_iter()
        .filter_map(|id| discussions.remove(&id))
        // CMS-FR-15.
        .filter(|d| !d.comments.is_empty())
        .collect();
    // CMS-FR-23: fragment order, which is the order the rail stacks its cards in.
    // A whole-target log has no fragments, so this degenerates to `created_at`
    // ascending there (CMS-FR-58).
    out.sort_by(|a, b| {
        a.fragment_target
            .as_ref()
            .map(|x| x.start)
            .unwrap_or(0)
            .cmp(&b.fragment_target.as_ref().map(|x| x.start).unwrap_or(0))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}
