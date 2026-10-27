//! Path-convention inference and the content-marker tiebreak
//! (`specifications/core/ASC-artifact-scanning.md` ASC-FR-03, ASC-FR-04,
//! ASC-FR-19, ASC-FR-JWPH). Pure: the caller supplies the text it read.

use super::{normalize_rel, ArtifactType};

/// Infer an artifact type from path/location convention across agent
/// ecosystems (ASC-FR-03). Returns `None` when no recognizer matches — the
/// caller then tries the content tiebreak and finally folder inheritance.
///
/// The representative recognizer table below spans Claude Code
/// (`.claude/skills`, `.claude/agents`, `.claude/commands`), Codex
/// (`AGENTS.md`), GitHub Copilot (`.github/**instructions**`,
/// `.github/prompts`), and the synthesis convention (`specifications/`). It is
/// intentionally extensible; the *type set* (ASC-FR-02) is what is fixed.
pub fn infer_from_path(rel: &str) -> Option<ArtifactType> {
    let rel = normalize_rel(rel);
    let segs: Vec<&str> = rel.split('/').filter(|s| !s.is_empty()).collect();
    let base = segs.last().copied().unwrap_or("");
    let base_lower = base.to_ascii_lowercase();

    // Directory-convention recognizers: a window of two consecutive path
    // segments anchors the ecosystem (e.g. `.claude/skills`).
    if has_dir_pair(&segs, ".claude", "skills") {
        return Some(ArtifactType::Skill);
    }
    if has_dir_pair(&segs, ".claude", "agents") {
        return Some(ArtifactType::Agent);
    }
    if has_dir_pair(&segs, ".claude", "commands") {
        return Some(ArtifactType::Prompt);
    }
    if has_dir_pair(&segs, ".github", "prompts") {
        return Some(ArtifactType::Prompt);
    }

    // Filename-convention recognizers (ecosystem-agnostic).
    if base == "AGENTS.md" {
        return Some(ArtifactType::Agent);
    }
    if base == "CLAUDE.md" || base == "AGENT.md" {
        return Some(ArtifactType::Instructions);
    }
    if base_lower.ends_with(".instructions.md") {
        return Some(ArtifactType::Instructions);
    }
    // GitHub Copilot keeps instruction files under `.github/` (e.g.
    // `.github/copilot-instructions.md`).
    if segs.first() == Some(&".github") && base_lower.contains("instructions") {
        return Some(ArtifactType::Instructions);
    }
    if base_lower.ends_with(".prompt.md") {
        return Some(ArtifactType::Prompt);
    }
    // The Flow document convention (ASC-FR-03): a `.flow` file, whose contents
    // are JSON. Because they are JSON, the content tiebreak
    // (`read_content_facts`, which reads Markdown frontmatter) can never reach a
    // Flow — the extension and an explicit assignment are the only two things
    // that classify one, and the assignment wins (ASC-FR-06).
    // `.js.flow` is a Facebook Flow type-declaration file that happens to share
    // the suffix; a JS project committing one is not describing a workflow, and
    // opening it on the Flow canvas would only fail to parse (FLO-FR-05). A bare
    // `.flow` with no stem is a tool's dotfile on the same reasoning.
    if base_lower.ends_with(".flow") && base_lower != ".flow" && !base_lower.ends_with(".js.flow") {
        return Some(ArtifactType::Flow);
    }
    if base_lower.ends_with(".scenario.md") {
        return Some(ArtifactType::Scenario);
    }
    if base_lower.ends_with(".scratchpad.md") {
        return Some(ArtifactType::Scratchpad);
    }
    if base_lower.ends_with(".spec.md") {
        return Some(ArtifactType::Spec);
    }
    // Synthesis spec convention: anything under a top-level `specifications/`.
    if segs.first() == Some(&"specifications") && segs.len() > 1 {
        return Some(ArtifactType::Spec);
    }

    None
}

/// True if `a` immediately followed by `b` appears as a consecutive segment
/// pair anywhere in `segs` (e.g. `.claude/skills`).
fn has_dir_pair(segs: &[&str], a: &str, b: &str) -> bool {
    segs.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// What a file's leading frontmatter says about itself: the `type:` marker the
/// classification tiebreak reads (ASC-FR-04) and the `name:` a skill declares
/// (ASC-FR-19).
///
/// One shape for both because they come out of one read: the scan opens a file
/// at most once, and splitting them into two lookups would double that for
/// every file that has anything to say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContentFacts {
    pub artifact_type: Option<ArtifactType>,
    pub declared_name: Option<String>,
}

/// Read a leading `---` frontmatter block for the keys the scan cares about.
///
/// Deliberately a line walk rather than a YAML parse: this runs over every file
/// the tiebreak is consulted for, the two keys it reads are scalars, and a file
/// whose frontmatter does not parse must still classify by the keys that do —
/// there is no error to report here, only facts that are present or absent.
/// Only top-level keys count; an indented `name:` belongs to something else.
pub fn read_content_facts(text: &str) -> ContentFacts {
    let mut facts = ContentFacts::default();
    let mut lines = text.lines();
    // Frontmatter must be the very first line.
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return facts,
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break; // end of frontmatter
        }
        if line.starts_with([' ', '\t']) {
            continue; // nested under some other key, not the file's own
        }
        if let Some(rest) = trimmed.strip_prefix("type:").or_else(|| trimmed.strip_prefix("kind:"))
        {
            // Strip surrounding quotes a YAML author might add.
            let value = rest.trim().trim_matches(|c| c == '"' || c == '\'');
            if facts.artifact_type.is_none() {
                facts.artifact_type = ArtifactType::from_marker(value);
            }
        } else if let Some(rest) = trimmed.strip_prefix("name:") {
            let value = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
            // `name: |` / `name: >` open a block scalar whose value is on the
            // lines beneath. Taking the indicator itself would render a node as
            // "|" — worse than the filename this falls back to.
            if facts.declared_name.is_none() && !value.is_empty() && value != "|" && value != ">" {
                facts.declared_name = Some(value.to_string());
            }
        }
    }
    facts
}

/// Content-marker tiebreak (ASC-FR-04): the `type:` (or `kind:`) key of a
/// leading `---` frontmatter block. Only consulted when path inference is
/// ambiguous.
pub fn infer_from_content(text: &str) -> Option<ArtifactType> {
    read_content_facts(text).artifact_type
}

/// ASC-FR-19: whether `rel` is a file whose declared name identifies it, because
/// its filename does not. A skill lives in `SKILL.md`, so every skill in a
/// project shares one filename; nothing else in the built-in type set has that
/// problem.
pub(super) fn names_itself(rel: &str, artifact_type: Option<ArtifactType>) -> bool {
    artifact_type == Some(ArtifactType::Skill)
        && rel.rsplit('/').next().is_some_and(|base| base.eq_ignore_ascii_case("SKILL.md"))
}
