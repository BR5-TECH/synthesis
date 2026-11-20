//! The `.gitignore` filter on a watcher batch (ASC-FR-RSGA, BMI-FR-GJLW,
//! CHC-FR-AADZ).
//!
//! A build or a dev server writes into ignored folders (`target/`,
//! `node_modules/.vite`, `dist/`) many times a second. Those paths are outside
//! the tree (ASC-FR-09), outside every artifact index, and outside the Git
//! change set, so a batch of them alone must cause no work and show no
//! progress.

use std::collections::HashMap;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;

use crate::fs::{EntryKind, RootFs};
use crate::skills::SKILL_FOLDERS;

/// ASC-FR-RSGA: remove from `paths` every path that `.gitignore` excludes, on
/// the terms the scan walk applies (ASC-FR-09): nested `.gitignore` files, the
/// repository's `info/exclude`, and an ignored folder hiding all that is under
/// it. Returns how many paths it removed.
///
/// Two kinds of path stay whatever the rules say:
/// - a path in, or above, one of the five skill folders, because the skills
///   index enumerates them with no ignore rule (DSL-FR-06, DSL-FR-19);
/// - a path in, or above, `.synthesis/`, whose files this application owns and
///   whose consumers apply their own rules (ASC-FR-20).
pub fn drop_gitignored(root: &RootFs, paths: &mut Vec<String>) -> usize {
    let before = paths.len();
    let mut rules = IgnoreRules::new(root);
    paths.retain(|rel| always_kept(rel) || !rules.is_ignored(rel));
    before - paths.len()
}

fn always_kept(rel: &str) -> bool {
    in_or_above(rel, ".synthesis")
        || SKILL_FOLDERS.iter().any(|(folder, _)| in_or_above(rel, folder))
}

/// True when `rel` is `folder`, is inside it, or is one of its ancestors. An
/// ancestor counts because a delete of `.claude/` removes every skill under it.
fn in_or_above(rel: &str, folder: &str) -> bool {
    rel == folder
        || rel.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
        || folder.strip_prefix(rel).is_some_and(|rest| rest.starts_with('/'))
}

/// The ignore rules of one project root, read once per batch. A matcher is
/// read only for the folders the batch's paths pass through.
///
/// ASC-FR-TWVB: each path is matched by the rules of its nearest repository
/// root alone: the `.gitignore` files from that root down, and that root's own
/// `info/exclude`. A rule of an enclosing repository never applies below a
/// nested repository root.
pub(crate) struct IgnoreRules {
    root: RootFs,
    /// Folder (root-relative, `""` for the root) to its `.gitignore`, or
    /// `None` when it has none or it holds no rule.
    per_dir: HashMap<String, Option<Gitignore>>,
    /// Repository root to its `info/exclude` matcher.
    exclude: HashMap<String, Option<Gitignore>>,
    /// Folder to whether it is a repository root.
    is_repo: HashMap<String, bool>,
}

impl IgnoreRules {
    pub(crate) fn new(root: &RootFs) -> Self {
        IgnoreRules {
            root: root.clone(),
            per_dir: HashMap::new(),
            exclude: HashMap::new(),
            is_repo: HashMap::new(),
        }
    }

    /// True when `rel` or one of its ancestor folders below its repository
    /// root is ignored. A file in an ignored folder is ignored whatever a
    /// deeper `.gitignore` says, as Git and the scan walk both treat it.
    pub(crate) fn is_ignored(&mut self, rel: &str) -> bool {
        let parts: Vec<&str> = rel.split('/').collect();
        let mut repo = String::new();
        for end in 1..=parts.len() {
            let candidate = parts[..end].join("/");
            // An ancestor is a folder by definition. The path itself is a
            // folder only when one is there now; a deleted path has no kind
            // left to read, so it is matched as a file.
            let is_dir = end < parts.len() || self.is_dir(&candidate);
            if self.matches(&candidate, is_dir, &repo) {
                return true;
            }
            // ASC-FR-TWVB: below a nested repository root, only its rules.
            if end < parts.len() && self.is_repo_root(&candidate) {
                repo = candidate;
            }
        }
        false
    }

    /// The deepest `.gitignore` with an opinion on `rel` decides, then the
    /// `info/exclude` of `repo`. A `!` rule is a decision too: it keeps the
    /// path. No folder above `repo` is asked.
    fn matches(&mut self, rel: &str, is_dir: bool, repo: &str) -> bool {
        let path = self.root.join(rel);
        let mut dir = parent_of(rel);
        loop {
            if let Some(matcher) = self.matcher(dir) {
                match matcher.matched(&path, is_dir) {
                    Match::Ignore(_) => return true,
                    Match::Whitelist(_) => return false,
                    Match::None => {}
                }
            }
            if dir.is_empty() || dir == repo {
                break;
            }
            dir = parent_of(dir);
        }
        self.exclude_of(repo).is_some_and(|m| m.matched(&path, is_dir).is_ignore())
    }

    fn matcher(&mut self, dir: &str) -> Option<&Gitignore> {
        let root = &self.root;
        self.per_dir
            .entry(dir.to_string())
            .or_insert_with(|| {
                let file = if dir.is_empty() {
                    ".gitignore".to_string()
                } else {
                    format!("{dir}/.gitignore")
                };
                read_matcher(root, dir, &file)
            })
            .as_ref()
    }

    fn exclude_of(&mut self, repo: &str) -> Option<&Gitignore> {
        let root = &self.root;
        self.exclude
            .entry(repo.to_string())
            .or_insert_with(|| {
                let file = super::repos::exclude_file_of(root, repo)?;
                read_matcher_at(root, repo, &file)
            })
            .as_ref()
    }

    fn is_repo_root(&mut self, rel: &str) -> bool {
        let root = &self.root;
        *self
            .is_repo
            .entry(rel.to_string())
            .or_insert_with(|| super::repos::is_repo_root(root, rel))
    }

    fn is_dir(&self, rel: &str) -> bool {
        self.root.file_info(rel).is_ok_and(|info| info.kind == EntryKind::Dir)
    }
}

fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// A matcher rooted at `dir` from the rules in `file`. A missing or unreadable
/// file, and one with no rule in it, yield `None`. A line that does not parse
/// is skipped, as Git skips it.
fn read_matcher(root: &RootFs, dir: &str, file: &str) -> Option<Gitignore> {
    let text = root.read_text(file).ok()?;
    matcher_from(root, dir, &text)
}

/// ASC-FR-TWVB: a matcher rooted at `repo` from an `info/exclude` file, which
/// can be outside the project root (FSA-FR-19 exception, as in
/// `repos::exclude_file_of`).
fn read_matcher_at(root: &RootFs, repo: &str, file: &std::path::Path) -> Option<Gitignore> {
    let text = std::fs::read_to_string(file).ok()?;
    matcher_from(root, repo, &text)
}

fn matcher_from(root: &RootFs, dir: &str, text: &str) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new(root.join(dir));
    for line in text.lines() {
        let _ = builder.add_line(None, line);
    }
    builder.build().ok().filter(|m| !m.is_empty())
}
