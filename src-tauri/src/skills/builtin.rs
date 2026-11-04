//! Built-in skills (`DSL-dynamic-skills-loading.md` DSL-FR-WTPA through
//! DSL-FR-LFCA).
//!
//! The application ships skills of its own. `build.rs` compiles every file
//! under `resources/skills/` into the binary, so this module reads no disk:
//! the list below is the whole built-in source, fixed for one build.
//!
//! A built-in skill goes into the same registry and the same `skills` index as
//! the project skills, with the ecosystem `builtin` and a virtual path that no
//! project file can have (DSL-FR-YBDO).

use super::{eligible, parse_frontmatter, Ecosystem, SkillDescriptor, SKILL_FILE};

include!(concat!(env!("OUT_DIR"), "/builtin_skills.rs"));

/// DSL-FR-YBDO: the prefix of every built-in skill's path. The colon makes the
/// path virtual: the four project folders start with a `.`, and no
/// project-relative path that the registry produces holds a `:`.
pub const BUILTIN_PATH_PREFIX: &str = "builtin:";

/// Every embedded file, as `(path relative to resources/skills, text)`, sorted
/// by path.
pub fn files() -> &'static [(&'static str, &'static str)] {
    BUILTIN_SKILL_FILES
}

/// DSL-FR-YBDO: the virtual path of the built-in skill in `folder_name`.
pub fn virtual_path(folder_name: &str) -> String {
    format!("{BUILTIN_PATH_PREFIX}{folder_name}/{SKILL_FILE}")
}

/// DSL-FR-ZUFC / DSL-FR-UYMA: every eligible built-in skill, in path order,
/// together with the folder names of the built-in skills that are not
/// eligible. The application ships none of the second kind; a test of the
/// build fails when one appears.
pub fn enumerate_builtin() -> (Vec<SkillDescriptor>, Vec<String>) {
    let mut skills = Vec::new();
    let mut excluded = Vec::new();
    for (path, text) in files() {
        // DSL-FR-03 applied to the embedded tree: exactly one folder segment
        // and then `SKILL.md`. A `SKILL.md` deeper down is a supporting file.
        let mut parts = path.split('/');
        let (Some(folder_name), Some(SKILL_FILE), None) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        match parse_frontmatter(text) {
            Some(front) if eligible(&front).is_ok() => skills.push(SkillDescriptor {
                path: virtual_path(folder_name),
                ecosystem: Ecosystem::Builtin,
                folder_name: folder_name.to_string(),
                // DSL-FR-10: the folder name stands in for a missing name.
                name: front.name.clone().unwrap_or_else(|| folder_name.to_string()),
                description: front.description.clone().unwrap_or_default(),
            }),
            _ => excluded.push(folder_name.to_string()),
        }
    }
    skills.sort_by(|a, b| a.path.cmp(&b.path));
    (skills, excluded)
}

/// DSL-FR-LFCA: the embedded text of `file`, relative to the folder of the
/// built-in skill `folder_name`.
///
/// `None` for a folder that holds no built-in skill, for a file the build does
/// not hold, and for a path that is absolute or holds a `..` component. `.`
/// segments and repeated separators are dropped, so `./references/a.md` and
/// `references//a.md` name the same file as `references/a.md`.
pub fn builtin_skill_file(folder_name: &str, file: &str) -> Option<&'static str> {
    let segments = relative_segments(file)?;
    if segments.is_empty() || folder_name.is_empty() || folder_name.contains('/') {
        return None;
    }
    let wanted = format!("{folder_name}/{}", segments.join("/"));
    let skill_file = format!("{folder_name}/{SKILL_FILE}");
    // A folder without a `SKILL.md` holds no built-in skill (DSL-FR-WTPA).
    if !files().iter().any(|(path, _)| *path == skill_file) {
        return None;
    }
    files().iter().find(|(path, _)| *path == wanted).map(|(_, text)| *text)
}

/// The segments of a relative path, or `None` when the path is absolute or
/// climbs with `..`. Both separators count, so a path written with `\` cannot
/// slip a `..` past this check.
pub fn relative_segments(file: &str) -> Option<Vec<&str>> {
    let file = file.trim();
    if file.starts_with('/') || file.starts_with('\\') || std::path::Path::new(file).is_absolute() {
        return None;
    }
    // A Windows drive prefix such as `C:` is absolute on that platform alone;
    // refuse it everywhere so the rule does not depend on the build target.
    if file.len() >= 2 && file.as_bytes()[1] == b':' {
        return None;
    }
    let mut segments = Vec::new();
    for segment in file.split(['/', '\\']) {
        match segment {
            "" | "." => continue,
            ".." => return None,
            other => segments.push(other),
        }
    }
    Some(segments)
}
