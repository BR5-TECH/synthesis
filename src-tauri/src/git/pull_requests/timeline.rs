//! The timeline of a pull request: GitHub's events mapped to the items of
//! GTC-FR-CKTM, review comments counted once, oldest first.

use std::collections::HashSet;

use serde_json::Value;

use super::model::{nested_text, text, PullRequestTimelineItem};

const KIND_COMMENT: &str = "comment";
const KIND_REVIEW: &str = "review";
const KIND_REVIEW_COMMENT: &str = "review_comment";
const KIND_COMMIT: &str = "commit";
const KIND_EVENT: &str = "event";

/// Text that is present and not empty. An empty body is no body.
fn non_empty(value: &Value, key: &str) -> Option<String> {
    text(value, key).filter(|body| !body.is_empty())
}

/// The login of the account in `value[key]`, when there is one.
fn login(value: &Value, key: &str) -> Option<String> {
    nested_text(value, key, "login")
}

fn numeric_id(value: &Value) -> Option<u64> {
    value.get("id").and_then(Value::as_u64)
}

/// The review states of the contract. GitHub spells them in lower case on the
/// timeline and in upper case on the review endpoints.
fn review_state(value: &Value) -> Option<String> {
    let state = text(value, "state")?.to_ascii_lowercase();
    matches!(
        state.as_str(),
        "approved" | "changes_requested" | "commented" | "dismissed" | "pending"
    )
    .then_some(state)
}

/// One review comment: a comment on a line of a file in the diff.
/// `None` for an entry with no id, which could not be told apart from another.
fn review_comment(value: &Value) -> Option<(u64, PullRequestTimelineItem)> {
    let id = numeric_id(value)?;
    let item = PullRequestTimelineItem {
        id: format!("{KIND_REVIEW_COMMENT}:{id}"),
        kind: KIND_REVIEW_COMMENT.to_string(),
        actor: login(value, "user").or_else(|| login(value, "actor")),
        created_at: text(value, "created_at").unwrap_or_default(),
        body: non_empty(value, "body"),
        path: text(value, "path"),
        ..Default::default()
    };
    Some((id, item))
}

/// A `committed` event. GitHub reports no account for it, only the names in the
/// commit itself, so the actor is the author's name. No email is carried over.
fn commit_item(event: &Value) -> Option<PullRequestTimelineItem> {
    let sha = text(event, "sha")?;
    let message = text(event, "message").unwrap_or_default();
    let (subject, rest) = match message.split_once('\n') {
        Some((first, rest)) => (first, rest.trim()),
        None => (message.as_str(), ""),
    };
    let created_at = nested_text(event, "committer", "date")
        .or_else(|| nested_text(event, "author", "date"))
        .or_else(|| text(event, "created_at"))
        .unwrap_or_default();
    Some(PullRequestTimelineItem {
        id: format!("{KIND_COMMIT}:{sha}"),
        kind: KIND_COMMIT.to_string(),
        actor: nested_text(event, "author", "name"),
        created_at,
        body: (!rest.is_empty()).then(|| rest.to_string()),
        commit_id: Some(sha),
        subject: Some(subject.to_string()),
        ..Default::default()
    })
}

/// Any activity the contract has no kind of its own for: `event` carries
/// GitHub's own name for it.
fn plain_event(event: &Value, name: &str, index: usize) -> PullRequestTimelineItem {
    let created_at = text(event, "created_at").unwrap_or_default();
    // The position is only the last resort, for an event GitHub gave no id.
    let id = match numeric_id(event) {
        Some(id) => format!("{KIND_EVENT}:{id}"),
        None => match text(event, "node_id") {
            Some(node) => format!("{KIND_EVENT}:{node}"),
            None => format!("{KIND_EVENT}:{name}:{created_at}:{index}"),
        },
    };
    PullRequestTimelineItem {
        id,
        kind: KIND_EVENT.to_string(),
        actor: login(event, "actor"),
        created_at,
        event: Some(name.to_string()),
        ..Default::default()
    }
}

/// The items one timeline event stands for. Most events are one item; a
/// `line-commented` event holds a thread of review comments and is each of them.
fn map_event(
    event: &Value,
    index: usize,
    seen_comments: &mut HashSet<u64>,
) -> Vec<PullRequestTimelineItem> {
    let Some(name) = text(event, "event") else {
        return Vec::new();
    };
    match name.as_str() {
        "commented" => {
            let Some(id) = numeric_id(event) else {
                return vec![plain_event(event, &name, index)];
            };
            vec![PullRequestTimelineItem {
                id: format!("{KIND_COMMENT}:{id}"),
                kind: KIND_COMMENT.to_string(),
                actor: login(event, "actor").or_else(|| login(event, "user")),
                created_at: text(event, "created_at").unwrap_or_default(),
                body: non_empty(event, "body"),
                ..Default::default()
            }]
        }
        "reviewed" => {
            let Some(id) = numeric_id(event) else {
                return vec![plain_event(event, &name, index)];
            };
            vec![PullRequestTimelineItem {
                id: format!("{KIND_REVIEW}:{id}"),
                kind: KIND_REVIEW.to_string(),
                actor: login(event, "user").or_else(|| login(event, "actor")),
                created_at: text(event, "submitted_at")
                    .or_else(|| text(event, "created_at"))
                    .unwrap_or_default(),
                body: non_empty(event, "body"),
                review_state: review_state(event),
                ..Default::default()
            }]
        }
        "line-commented" => event
            .get("comments")
            .and_then(Value::as_array)
            .map(|comments| {
                comments
                    .iter()
                    .filter_map(review_comment)
                    .filter(|(id, _)| seen_comments.insert(*id))
                    .map(|(_, item)| item)
                    .collect()
            })
            .unwrap_or_default(),
        "committed" => commit_item(event)
            .map(|item| vec![item])
            .unwrap_or_else(|| vec![plain_event(event, &name, index)]),
        _ => vec![plain_event(event, &name, index)],
    }
}

/// Seconds since the Unix epoch of an RFC 3339 time, offset applied. `None` for
/// text that is not one.
fn instant(time: &str) -> Option<i64> {
    let (date, clock) = time.split_once(['T', 't', ' '])?;
    let mut date_parts = date.splitn(3, '-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // The offset starts at `Z`, `+` or `-`; the clock itself has none of them.
    let zone_at = clock.find(['Z', 'z', '+', '-'])?;
    let (clock, zone) = clock.split_at(zone_at);
    let mut clock_parts = clock.splitn(3, ':');
    let hour: i64 = clock_parts.next()?.parse().ok()?;
    let minute: i64 = clock_parts.next()?.parse().ok()?;
    let second: i64 = clock_parts.next()?.split('.').next()?.parse().ok()?;
    let offset = match zone.as_bytes().first()? {
        b'Z' | b'z' => 0,
        sign => {
            let (h, m) = zone[1..].split_once(':')?;
            let seconds = h.parse::<i64>().ok()? * 3600 + m.parse::<i64>().ok()? * 60;
            if *sign == b'-' { -seconds } else { seconds }
        }
    };
    // Days from the civil date (proleptic Gregorian).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// The items of the timeline: every event mapped, the review comments of
/// `review_comments` added when the events did not already carry them, oldest
/// first. The sort is stable, so items of one instant keep GitHub's order.
pub(super) fn build_timeline(
    events: &[Value],
    review_comments: &[Value],
) -> Vec<PullRequestTimelineItem> {
    let mut seen_comments: HashSet<u64> = HashSet::new();
    let mut items: Vec<PullRequestTimelineItem> = events
        .iter()
        .enumerate()
        .flat_map(|(index, event)| map_event(event, index, &mut seen_comments))
        .collect();
    for (id, item) in review_comments.iter().filter_map(review_comment) {
        if seen_comments.insert(id) {
            items.push(item);
        }
    }
    items.sort_by_key(|item| instant(&item.created_at).unwrap_or(i64::MIN));
    items
}
