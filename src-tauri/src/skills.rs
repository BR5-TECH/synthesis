//! Dynamic skills loading (`DSL-dynamic-skills-loading.md`).
//!
//! The registry of skills an AI agent may reach for on its own: the four
//! folders the agent ecosystems keep their project skills in, walked directly,
//! each eligible `SKILL.md`'s declared name and description read out of its
//! frontmatter. The descriptors are published as the `skills` index of
//! `BMI-bm25-indexing.md` (DSL-FR-13) and answered from by [`list_skills`] and
//! [`search_skills`]. Nothing here is a `#[tauri::command]` (DSL-FR-01).
//!
//! ## Why this walks its own folders instead of using the scan
//!
//! Every other file set in the application comes from the scan's candidate list
//! (ASC-FR-17) and inherits its exclusions. This one does not, because a skill
//! an agent can invoke is present whether or not it is committed, and a project
//! that gitignores `.claude/` would otherwise have no skills at all (DSL-FR-06).
//! The cost of that choice is that the scan cannot be the trigger either: a
//! gitignored file's edit changes nothing in the tree. The watcher's raw path
//! channel (ASC-FR-20) is what reports it, and every pass re-enumerates all four
//! folders in full rather than interpreting the event (DSL-FR-19) — four shallow
//! directory listings, which is cheaper than being clever and cannot drift.
//!
//! ## Why a descriptor rather than the file
//!
//! An agent choosing a skill reads descriptions, not bodies. The same
//! `SKILL.md` is also chunked whole into the `skill` artifact index when the
//! scan classifies it that way, and the two indexes share no statistics
//! (DSL-FR-14) — so ranking a skill on how it describes itself is not disturbed
//! by how much it wrote.

use std::path::Path;

use serde::Serialize;

use crate::bm25_index::{self, Bm25Indexer, IndexId, MAX_FILE_BYTES};

// ---------------------------------------------------------------------------
// The four special folders (DSL-FR-02)
// ---------------------------------------------------------------------------

/// Which of the four folder families a skill was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Claude,
    Codex,
    Github,
    Opencode,
}

impl Ecosystem {
    pub fn as_str(self) -> &'static str {
        match self {
            Ecosystem::Claude => "claude",
            Ecosystem::Codex => "codex",
            Ecosystem::Github => "github",
            Ecosystem::Opencode => "opencode",
        }
    }
}

/// DSL-FR-02: the four folders, relative to the project root and matched there
/// alone. A package's own `.claude/skills/` deeper in a monorepo is not one of
/// them, so two packages cannot collide over a skill name.
pub const SKILL_FOLDERS: [(&str, Ecosystem); 4] = [
    (".claude/skills", Ecosystem::Claude),
    (".codex/skills", Ecosystem::Codex),
    (".github/skills", Ecosystem::Github),
    (".opencode/skills", Ecosystem::Opencode),
];

/// DSL-FR-03: the one basename that identifies a skill, compared
/// case-sensitively against the name on disk so `skill.md` is ignored even on a
/// filesystem that would happily open it under either name.
pub const SKILL_FILE: &str = "SKILL.md";

// ---------------------------------------------------------------------------
// Contract surface types
// ---------------------------------------------------------------------------

/// One eligible skill (DSL-FR-12).
///
/// Carries no scan node id: a skill is eligible whether or not the scan
/// surfaced its file (DSL-FR-06, DSL-FR-11), so `path` is the only identity it
/// always has — and it is what tells two skills of the same name apart
/// (DSL-FR-17).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDescriptor {
    /// Project-relative path of the `SKILL.md`.
    pub path: String,
    pub ecosystem: Ecosystem,
    /// The `<skill-name>` folder segment.
    pub folder_name: String,
    /// The declared frontmatter name, or `folder_name` (DSL-FR-10).
    pub name: String,
    /// The declared frontmatter description (DSL-FR-09).
    pub description: String,
}

impl SkillDescriptor {
    /// DSL-FR-13: the document this skill is indexed as — its name and its
    /// description, and nothing else. The body below the frontmatter
    /// contributes nothing.
    pub fn document(&self) -> String {
        format!("{}\n\n{}", self.name, self.description)
    }
}

/// One ranked skill returned by [`search_skills`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedSkill {
    pub skill: SkillDescriptor,
    /// BM25 score against the `skills` index (BMI-FR-11).
    pub score: f32,
}

/// A file at an eligible path that was excluded, and why (DSL-FR-24).
///
/// The reason is a fixed phrase rather than anything read out of the file: a
/// `SKILL.md` is user content, and no part of it belongs in a log record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exclusion {
    pub path: String,
    pub reason: &'static str,
}

pub const REASON_SYMLINK: &str = "refusing to read through a symlink";
pub const REASON_NO_FRONTMATTER: &str = "no leading frontmatter block";
pub const REASON_DISABLED: &str = "disable-model-invocation is set";
pub const REASON_NO_DESCRIPTION: &str = "no description declared";
pub const REASON_TOO_LARGE: &str = "exceeds the size ceiling";
pub const REASON_NOT_TEXT: &str = "not valid UTF-8 text";

// ---------------------------------------------------------------------------
// Frontmatter (DSL-FR-07, DSL-FR-08, DSL-FR-09, DSL-FR-10)
// ---------------------------------------------------------------------------

/// The three keys this module reads out of a skill's frontmatter. Every other
/// key a skill declares is ignored rather than rejected, so a skill carrying an
/// ecosystem-specific key its own CLI understands is still eligible here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    pub disable_model_invocation: bool,
    /// Whether the key was seen at all, so a second declaration of it cannot
    /// override the first (see [`store`]).
    saw_disable_key: bool,
}

/// Parse a leading `---` frontmatter block, or `None` when there is none.
///
/// A line walk rather than a YAML parse, matching `scanning::read_content_facts`
/// — the keys are scalars, and pulling a YAML dependency in to read three of
/// them would buy nothing. Block scalars (`>`, `>-`, `|`, `|-`) are folded,
/// because a description long enough to want one is exactly the description
/// worth indexing.
///
/// `None` is the DSL-FR-07 exclusion: no opening `---` on the very first line,
/// or no closing one at all. A block that opens and closes parses even if a key
/// inside it is malformed — a skill is not excluded for a key this module does
/// not read.
pub fn parse_frontmatter(text: &str) -> Option<Frontmatter> {
    // A UTF-8 BOM is part of the first line as far as `str::lines` is
    // concerned, so without this a skill authored on Windows — where editors
    // emit one routinely — would be excluded for having "no frontmatter" while
    // looking perfectly correct in every editor its author opens it in.
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    match lines.next() {
        Some(l) if l.trim_end() == "---" => {}
        _ => return None,
    }
    let mut front = Frontmatter::default();
    let mut closed = false;
    // Set while folding a block scalar, holding the key it belongs to.
    let mut block: Option<(&'static str, Vec<String>)> = None;

    for line in lines {
        let trimmed = line.trim();
        let indented = line.starts_with([' ', '\t']);

        if trimmed == "---" && !indented {
            closed = true;
            break;
        }
        // A block scalar's value is the indented run beneath it; a blank line
        // inside one is part of it.
        if let Some((key, collected)) = block.as_mut() {
            if indented || trimmed.is_empty() {
                collected.push(trimmed.to_string());
                continue;
            }
            let key = *key;
            let value = fold(std::mem::take(collected));
            store(&mut front, key, &value);
            block = None;
        }
        if indented {
            continue; // nested under some other key, not the file's own
        }

        for key in ["name", "description", "disable-model-invocation"] {
            let Some(rest) = trimmed.strip_prefix(key).and_then(|r| r.strip_prefix(':')) else {
                continue;
            };
            let value = rest.trim();
            if matches!(value, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
                block = Some((key_name(key), Vec::new()));
            } else {
                store(&mut front, key_name(key), unquote(value));
            }
            break;
        }
    }
    if let Some((key, collected)) = block {
        let value = fold(collected);
        store(&mut front, key, &value);
    }
    // DSL-FR-07: an unterminated block is not frontmatter, it is a file that
    // happens to start with `---`. Excluding it here is what keeps a Markdown
    // horizontal rule on line one from being read as a declaration.
    closed.then_some(front)
}

/// A `&'static str` for a key, so the block-scalar state can hold it without
/// borrowing the line it came from.
fn key_name(key: &str) -> &'static str {
    match key {
        "name" => "name",
        "description" => "description",
        _ => "disable-model-invocation",
    }
}

/// Record a key's value, first declaration winning.
///
/// All three keys are first-wins. A file that declares one of them twice is
/// malformed YAML either way, and the alternative — first-wins for the two
/// strings and last-wins for the boolean — would mean a skill declaring
/// `disable-model-invocation` twice with different values resolved by which
/// came last, which is not a rule anyone could predict.
///
/// The blank check trims, so `name: "   "` is treated as absent rather than
/// stored: DSL-FR-10 says a blank name falls back to the folder segment, and a
/// descriptor whose `name` is three spaces satisfies neither the fallback nor
/// the declaration.
fn store(front: &mut Frontmatter, key: &'static str, value: &str) {
    let value = value.trim();
    match key {
        "name" => {
            if front.name.is_none() && !value.is_empty() {
                front.name = Some(value.to_string());
            }
        }
        "description" => {
            if front.description.is_none() && !value.is_empty() {
                front.description = Some(value.to_string());
            }
        }
        // DSL-FR-08: read as a boolean, with the string `"true"` counting as
        // one however it is cased. Every other value, and the key's absence,
        // leaves the skill included.
        _ => {
            if !front.saw_disable_key {
                front.saw_disable_key = true;
                front.disable_model_invocation = value.eq_ignore_ascii_case("true");
            }
        }
    }
}

/// Join a block scalar's lines into one, which is all the indexed document
/// needs — the distinction between folded and literal is about rendering, and
/// nothing renders a description here.
fn fold(lines: Vec<String>) -> String {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip the quotes a YAML author might add around a scalar.
fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    for quote in ['"', '\''] {
        if trimmed.len() >= 2 && trimmed.starts_with(quote) && trimmed.ends_with(quote) {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

// ---------------------------------------------------------------------------
// Enumeration (DSL-FR-02 through DSL-FR-11, DSL-FR-23, DSL-FR-25)
// ---------------------------------------------------------------------------

/// Walk the four special folders under `root` and return every eligible skill,
/// together with the files that sat at an eligible path but were excluded.
///
/// Pure over the filesystem alone — no Tauri runtime — so every eligibility
/// claim is testable against a temp directory. Deterministic: the folders are
/// walked in a fixed order and the result is sorted by path, so the same tree
/// enumerated twice yields the same registry (DSL-FR-25).
pub fn enumerate(root: &crate::fs::RootFs) -> (Vec<SkillDescriptor>, Vec<Exclusion>) {
    let mut skills = Vec::new();
    let mut excluded = Vec::new();

    for (folder, ecosystem) in SKILL_FOLDERS {
        let abs = root.join(folder);
        let Ok(meta) = root.file_info(&abs) else {
            // DSL-FR-06: an absent special folder yields no skills and is not
            // an error. Nor is one that cannot be read.
            continue;
        };
        if meta.kind == crate::fs::EntryKind::Symlink {
            // DSL-FR-05: a symlinked special folder contributes no skills at
            // all, and nothing is resolved through it.
            excluded.push(Exclusion {
                path: folder.to_string(),
                reason: REASON_SYMLINK,
            });
            continue;
        }
        if meta.kind != crate::fs::EntryKind::Dir {
            continue;
        }
        let Ok(entries) = root.list_dir(&abs) else {
            continue;
        };
        // `list_dir` already sorts by name (FSA-FR-22), so the registry's order
        // is the same on every machine rather than readdir's.
        let names: Vec<String> = entries.into_iter().map(|e| e.name).collect();

        for folder_name in names {
            let skill_dir = abs.join(&folder_name);
            let Ok(dir_meta) = root.file_info(&skill_dir) else {
                continue;
            };
            if dir_meta.kind == crate::fs::EntryKind::Symlink {
                // DSL-FR-05: a `<skill-name>` folder symlinked at a shared
                // library of skills outside the project contributes nothing.
                excluded.push(Exclusion {
                    path: format!("{folder}/{folder_name}"),
                    reason: REASON_SYMLINK,
                });
                continue;
            }
            if dir_meta.kind != crate::fs::EntryKind::Dir {
                // DSL-FR-03: `.claude/skills/foo.md` is not a skill. Ignored
                // rather than reported — a project has far more files that are
                // merely not skills than skills, and DSL-FR-24 keeps the log
                // for the ones that meant to be.
                continue;
            }
            // DSL-FR-03: exactly one folder segment, and the basename compared
            // against the name on disk. Reading the directory rather than
            // probing `skill_dir.join("SKILL.md")` is what makes the comparison
            // case-sensitive on a case-insensitive filesystem.
            let Ok(inner) = root.list_dir(&skill_dir) else {
                continue;
            };
            if !inner.iter().any(|e| e.name == SKILL_FILE) {
                continue;
            }
            let rel = format!("{folder}/{folder_name}/{SKILL_FILE}");
            let read = resolve_skill(root, &rel).and_then(|abs| read_skill(root, &abs));
            match read {
                Ok(front) => match eligible(&front) {
                    Ok(()) => skills.push(SkillDescriptor {
                        // DSL-FR-10: a skill that declares no usable name goes
                        // by its folder, rather than being excluded for it.
                        name: front
                            .name
                            .clone()
                            .unwrap_or_else(|| folder_name.clone()),
                        description: front.description.clone().unwrap_or_default(),
                        path: rel,
                        ecosystem,
                        folder_name,
                    }),
                    Err(reason) => excluded.push(Exclusion { path: rel, reason }),
                },
                Err(reason) => excluded.push(Exclusion { path: rel, reason }),
            }
        }
    }

    skills.sort_by(|a, b| a.path.cmp(&b.path));
    (skills, excluded)
}

/// Read a `SKILL.md` and parse its frontmatter, refusing a symlink and anything
/// over the ceiling (DSL-FR-05, DSL-FR-23).
fn read_skill(root: &crate::fs::RootFs, abs: &Path) -> Result<Frontmatter, &'static str> {
    // The link itself is inspected rather than its target: a `SKILL.md`
    // symlinked at a file elsewhere on the machine would otherwise put that
    // file's frontmatter into the registry, and `search_skills` returns a
    // descriptor verbatim.
    let link = root.file_info(abs).map_err(|_| REASON_SYMLINK)?;
    if link.kind == crate::fs::EntryKind::Symlink {
        return Err(REASON_SYMLINK);
    }
    if link.kind != crate::fs::EntryKind::File {
        return Err(REASON_NO_FRONTMATTER);
    }
    if link.size > MAX_FILE_BYTES {
        return Err(REASON_TOO_LARGE);
    }
    // DSL-FR-23: through the FSA gate, so a file that does not decode as UTF-8
    // is excluded rather than fatal.
    let text = root.read_text(abs).map_err(|_| REASON_NOT_TEXT)?;
    parse_frontmatter(&text).ok_or(REASON_NO_FRONTMATTER)
}

/// Resolve one skill's `SKILL.md` under `root` through the FSA gate, so
/// FSA-FR-10's path-escape rejection binds this read too (DSL-FR-23).
///
/// The components are read off the disk rather than supplied by a caller, so
/// there is no `..` to reject in practice — but a directory entry's name is
/// still filesystem input, and routing it through the same gate every other
/// read in the application uses costs nothing and keeps the claim true.
fn resolve_skill(root: &Path, rel: &str) -> Result<std::path::PathBuf, &'static str> {
    crate::fs::resolve_under(root, rel).map_err(|_| REASON_SYMLINK)
}

/// The two declarations that exclude an otherwise well-placed skill.
fn eligible(front: &Frontmatter) -> Result<(), &'static str> {
    if front.disable_model_invocation {
        return Err(REASON_DISABLED);
    }
    // DSL-FR-09: a skill that says nothing about itself cannot be ranked
    // against one that does, and offering it unranked would put it ahead of
    // skills that earned their place.
    match front.description.as_deref() {
        Some(d) if !d.trim().is_empty() => Ok(()),
        _ => Err(REASON_NO_DESCRIPTION),
    }
}

// ---------------------------------------------------------------------------
// Retrieval (DSL-FR-15, DSL-FR-16)
// ---------------------------------------------------------------------------

/// DSL-FR-15: every eligible skill in the active worktree, in path order.
///
/// Returns a list in every circumstance rather than an error — no project open,
/// no special folder present, and every candidate excluded each yield an empty
/// list. Answers from the published snapshot, so it never blocks on a pass in
/// flight, and never disagrees with [`search_skills`] (DSL-FR-18): both read
/// one snapshot, in which the registry and the `skills` index were published
/// together.
pub fn list_skills(indexer: &Bm25Indexer) -> Vec<SkillDescriptor> {
    indexer.snapshot().skills().to_vec()
}

/// DSL-FR-16: the `limit` best-matching skills for `query`, ordered by
/// descending score.
///
/// Scores against the `skills` index alone and never against another, so a term
/// that appears only in some skill's body ranks nothing here (DSL-FR-14).
/// Returns a list in every circumstance rather than an error.
pub fn search_skills(indexer: &Bm25Indexer, query: &str, limit: usize) -> Vec<RankedSkill> {
    let snapshot = indexer.snapshot();
    bm25_index::search_snapshot(&snapshot, &[IndexId::Skills], query, limit)
        .into_iter()
        .filter_map(|hit| {
            snapshot
                .skills()
                .iter()
                .find(|s| s.path == hit.path)
                .map(|skill| RankedSkill {
                    skill: skill.clone(),
                    score: hit.score,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests;
