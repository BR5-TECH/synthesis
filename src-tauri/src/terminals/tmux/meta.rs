//! The name and the metadata of a persistent session in tmux (TRS-FR-BHCB,
//! TRS-FR-QMZM).

use sha2::{Digest, Sha256};

use crate::terminals::model::*;

/// TRS-FR-BHCB: the start of every session name of one project.
pub fn project_prefix(project_key: &str) -> String {
    let digest = Sha256::digest(project_key.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("syn-{}-", &hex[..12])
}

/// TRS-FR-BHCB: the tmux session name of one session.
pub fn session_name(project_key: &str, terminal_id: &str) -> String {
    format!("{}{terminal_id}", project_prefix(project_key))
}

/// TRS-FR-QMZM: what a tmux session holds about its session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub project: String,
    pub label: String,
    pub target: TargetRef,
    pub kind: TerminalKind,
    pub vendor: Option<CliVendor>,
    pub runs_in: RunsIn,
    pub pinned: bool,
    pub created_at: String,
}

/// The option names, in the order of [`list_format`] after the session name.
const KEYS: [&str; 8] = [
    "@synthesis_project",
    "@synthesis_label",
    "@synthesis_target",
    "@synthesis_kind",
    "@synthesis_vendor",
    "@synthesis_runs_in",
    "@synthesis_pinned",
    "@synthesis_created_at",
];

pub const PINNED_KEY: &str = "@synthesis_pinned";

/// Keeps a value on one field of the list output: `%`, tab, line feed, and
/// carriage return are written as `%XX`.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '%' => out.push_str("%25"),
            '\t' => out.push_str("%09"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            ';' => out.push_str("%3B"),
            _ => out.push(c),
        }
    }
    out
}

pub fn decode(value: &str) -> String {
    value
        .replace("%09", "\t")
        .replace("%0A", "\n")
        .replace("%0D", "\r")
        .replace("%3B", ";")
        .replace("%25", "%")
}

fn code_of<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn from_code<T: serde::de::DeserializeOwned>(code: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(code.to_string())).ok()
}

fn target_code(target: &TargetRef) -> String {
    match target {
        TargetRef::Worktree { path } => format!("worktree:{path}"),
        TargetRef::WorkStream { stream_id } => format!("work_stream:{stream_id}"),
    }
}

fn target_from(code: &str) -> Option<TargetRef> {
    if let Some(path) = code.strip_prefix("worktree:") {
        return Some(TargetRef::Worktree { path: path.to_string() });
    }
    code.strip_prefix("work_stream:").map(|id| TargetRef::WorkStream { stream_id: id.to_string() })
}

impl Meta {
    /// The option values, in the order of `KEYS`.
    fn values(&self) -> [String; 8] {
        [
            self.project.clone(),
            self.label.clone(),
            target_code(&self.target),
            code_of(self.kind),
            self.vendor.map(|v| v.id().to_string()).unwrap_or_default(),
            code_of(self.runs_in),
            if self.pinned { "1".into() } else { "0".into() },
            self.created_at.clone(),
        ]
    }

    /// TRS-FR-QMZM: one `set-option` for each value, joined into a command list.
    pub fn set_commands(&self, session: &str) -> Vec<Vec<String>> {
        KEYS.iter()
            .zip(self.values())
            .map(|(key, value)| super::commands::set_option(session, key, &encode(&value)))
            .collect()
    }

    fn parse(fields: &[&str]) -> Option<Self> {
        let [project, label, target, kind, vendor, runs_in, pinned, created_at] = fields else {
            return None;
        };
        Some(Meta {
            project: decode(project),
            label: decode(label),
            target: target_from(&decode(target))?,
            kind: from_code(kind)?,
            vendor: if vendor.is_empty() { None } else { Some(CliVendor::from_id(vendor)?) },
            runs_in: from_code(runs_in)?,
            pinned: *pinned == "1",
            created_at: decode(created_at),
        })
    }
}

/// TRS-FR-KBOS: the state of a session's pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneState {
    Alive,
    /// The process ended, with its exit status where tmux knows it.
    Dead(Option<i32>),
}

/// The format of `list-sessions -F`: the name, the metadata, and the pane.
pub fn list_format() -> String {
    let mut fields = vec!["#{session_name}".to_string()];
    fields.extend(KEYS.iter().map(|k| format!("#{{{k}}}")));
    fields.push(PANE_FORMAT.to_string());
    fields.join("\t")
}

/// The format of the pane state: whether it is dead, a tab, and its status.
pub const PANE_FORMAT: &str = "#{pane_dead}\t#{pane_dead_status}";

/// The pane state from the output of [`PANE_FORMAT`].
pub fn parse_pane(text: &str) -> Option<PaneState> {
    let mut parts = text.trim_end_matches(['\n', '\r']).splitn(2, '\t');
    let dead = parts.next()?;
    let status = parts.next().unwrap_or("");
    match dead {
        "1" => Some(PaneState::Dead(status.trim().parse().ok())),
        "0" => Some(PaneState::Alive),
        _ => None,
    }
}

/// One tmux session of a project, as `list-sessions` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub terminal_id: String,
    pub meta: Meta,
    pub pane: PaneState,
}

/// TRS-FR-IEKU: the sessions of `project_key` in the output of
/// `list-sessions -F list_format()`. A line of another project, of another
/// program, or with metadata that does not parse is left out.
pub fn parse_list(text: &str, project_key: &str) -> Vec<Listed> {
    let prefix = project_prefix(project_key);
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.len() != 1 + KEYS.len() + 2 {
                return None;
            }
            let name = fields[0];
            let terminal_id = name.strip_prefix(&prefix)?;
            if terminal_id.is_empty() {
                return None;
            }
            let meta = Meta::parse(&fields[1..=KEYS.len()])?;
            if meta.project != project_key {
                return None;
            }
            let pane = parse_pane(&fields[KEYS.len() + 1..].join("\t"))?;
            Some(Listed {
                name: name.to_string(),
                terminal_id: terminal_id.to_string(),
                meta,
                pane,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn meta(project: &str) -> Meta {
        Meta {
            project: project.into(),
            label: "Host shell 1".into(),
            target: TargetRef::Worktree { path: "/repo/a b".into() },
            kind: TerminalKind::HostShell,
            vendor: None,
            runs_in: RunsIn::Host,
            pinned: true,
            created_at: "2026-11-22T10:00:00Z".into(),
        }
    }

    fn line(name: &str, meta: &Meta, pane: &str) -> String {
        let mut fields = vec![name.to_string()];
        fields.extend(meta.values().iter().map(|v| encode(v)));
        fields.push(pane.to_string());
        fields.join("\t")
    }

    // TRS-FR-BHCB: the name holds a hash of the project key and the id, so a
    // key with spaces and dots gives a name that tmux accepts.
    #[test]
    fn names_hold_the_project_hash_and_the_id() {
        let name = session_name("/Users/me/my.repo", "t1a2b");
        assert!(name.starts_with("syn-"));
        assert!(name.ends_with("-t1a2b"));
        assert_eq!(name.len(), "syn-".len() + 12 + 1 + "t1a2b".len());
        assert!(!name.contains('.') && !name.contains(':') && !name.contains(' '));
        assert_ne!(project_prefix("/a"), project_prefix("/b"));
    }

    // TRS-FR-QMZM, TRS-FR-IEKU: the metadata goes into tmux and comes back the
    // same, for a worktree, a work stream, and an Agent CLI.
    #[test]
    fn metadata_round_trips() {
        let mut agent = meta("/p");
        agent.target = TargetRef::WorkStream { stream_id: "s-1".into() };
        agent.kind = TerminalKind::AgentCli;
        agent.vendor = Some(CliVendor::ClaudeCode);
        agent.pinned = false;
        agent.label = "50%\ttab".into();
        let text = format!(
            "{}\n{}\n",
            line(&session_name("/p", "t1"), &meta("/p"), "0\t"),
            line(&session_name("/p", "t2"), &agent, "1\t3"),
        );
        let listed = parse_list(&text, "/p");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].terminal_id, "t1");
        assert_eq!(listed[0].meta, meta("/p"));
        assert_eq!(listed[0].pane, PaneState::Alive);
        assert_eq!(listed[1].meta, agent);
        assert_eq!(listed[1].pane, PaneState::Dead(Some(3)));
    }

    // TRS-FR-ZOZO, TRS-FR-IEKU: a session of another project, a session with a
    // hash that matches but another key, and a foreign session are left out.
    #[test]
    fn only_the_sessions_of_the_project_are_listed() {
        let text = [
            line(&session_name("/other", "t1"), &meta("/other"), "0\t"),
            line(&session_name("/p", "t2"), &meta("/not-p"), "0\t"),
            "main\t\t\t\t\t\t\t\t\t0\t".to_string(),
            line(&project_prefix("/p"), &meta("/p"), "0\t"),
            "garbage".to_string(),
        ]
        .join("\n");
        assert!(parse_list(&text, "/p").is_empty());
    }

    // TRS-FR-KBOS: the pane state of a live and of a dead pane.
    #[test]
    fn reads_the_pane_state() {
        assert_eq!(parse_pane("0\t\n"), Some(PaneState::Alive));
        assert_eq!(parse_pane("1\t127\n"), Some(PaneState::Dead(Some(127))));
        assert_eq!(parse_pane("1\t"), Some(PaneState::Dead(None)));
        assert_eq!(parse_pane(""), None);
    }

    // TRS-FR-QMZM: the values that tmux holds carry no tab or line break.
    #[test]
    fn encoding_keeps_one_field() {
        let raw = "a\tb\nc\r%d;e;";
        let encoded = encode(raw);
        assert!(!encoded.contains(['\t', '\n', '\r', ';']));
        assert_eq!(decode(&encoded), raw);
    }
}
