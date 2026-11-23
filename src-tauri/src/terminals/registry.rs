//! The sessions of one project (TRS-FR-AYCY, TRS-FR-LRYH, TRS-FR-TEHT).
//!
//! The registry is in memory only. A new application process starts with an
//! empty one, and a project close empties it. Persistent sessions come back
//! into it from tmux (TRS-FR-IEKU).

use std::sync::{Arc, Mutex, MutexGuard};

use super::model::*;
use super::session::{Session, SessionSpec};

/// TRS-FR-AYCY: the most sessions one project holds, in every state.
pub const MAX_SESSIONS: usize = 16;

#[derive(Default)]
pub struct Registry {
    inner: Mutex<RegistryInner>,
}

#[derive(Default)]
struct RegistryInner {
    sessions: Vec<Arc<Session>>,
    next_sequence: u64,
}

/// What a new session is, before the registry gives it an id and a label.
pub struct NewSession {
    pub target: TargetRef,
    pub target_name: String,
    pub branch: Option<String>,
    pub kind: TerminalKind,
    pub vendor: Option<CliVendor>,
    pub runs_in: RunsIn,
    pub busy_run_id: Option<String>,
    pub initial_state: TerminalState,
}

/// TRS-FR-LRYH: the name a label starts with.
pub fn label_name(kind: TerminalKind, vendor: Option<CliVendor>) -> &'static str {
    match (kind, vendor) {
        (TerminalKind::HostShell, _) => "Host shell",
        (TerminalKind::ContainerShell, _) => "Container shell",
        (TerminalKind::MicrosandboxShell, _) => "Microsandbox shell",
        (TerminalKind::AgentCli, Some(vendor)) => vendor.display_name(),
        (TerminalKind::AgentCli, None) => "Agent CLI",
    }
}

/// TRS-FR-LRYH: the lowest positive number no other label of the name holds.
pub fn next_label(name: &str, existing: &[String]) -> String {
    let prefix = format!("{name} ");
    let mut n = 1u32;
    loop {
        let candidate = format!("{prefix}{n}");
        if !existing.iter().any(|label| label == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, RegistryInner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Adds a session. Refuses past the limit with `terminal_limit_reached`.
    pub fn add(
        &self,
        id: String,
        created_at: String,
        new: NewSession,
    ) -> Result<Arc<Session>, TerminalError> {
        let mut inner = self.lock();
        if inner.sessions.len() >= MAX_SESSIONS {
            return Err(TerminalError::new("terminal_limit_reached"));
        }
        // TRS-FR-LRYH: the number looks at the current labels.
        let labels: Vec<String> = inner.sessions.iter().map(|s| s.label()).collect();
        let label = next_label(label_name(new.kind, new.vendor), &labels);
        Ok(Self::insert(&mut inner, id, label, created_at, new))
    }

    /// TRS-FR-IEKU, TRS-FR-TFAN: adds a restored session with its own id and
    /// label. Refuses past the limit with `terminal_limit_reached`, and gives
    /// `None` for an id that the registry holds already.
    pub fn restore(
        &self,
        id: String,
        label: String,
        created_at: String,
        new: NewSession,
    ) -> Result<Option<Arc<Session>>, TerminalError> {
        let mut inner = self.lock();
        if inner.sessions.iter().any(|s| s.spec.id == id) {
            return Ok(None);
        }
        if inner.sessions.len() >= MAX_SESSIONS {
            return Err(TerminalError::new("terminal_limit_reached"));
        }
        Ok(Some(Self::insert(&mut inner, id, label, created_at, new)))
    }

    fn insert(
        inner: &mut RegistryInner,
        id: String,
        label: String,
        created_at: String,
        new: NewSession,
    ) -> Arc<Session> {
        let sequence = inner.next_sequence;
        inner.next_sequence += 1;
        let session = Arc::new(Session::new(
            SessionSpec {
                id,
                label,
                target: new.target,
                target_name: new.target_name,
                branch: new.branch,
                kind: new.kind,
                vendor: new.vendor,
                runs_in: new.runs_in,
                created_at,
                sequence,
            },
            new.initial_state,
            new.busy_run_id,
        ));
        inner.sessions.push(session.clone());
        session
    }

    /// Whether the registry holds a session with `id`.
    pub fn contains(&self, id: &str) -> bool {
        self.lock().sessions.iter().any(|s| s.spec.id == id)
    }

    /// The number of sessions, in every state (TRS-FR-AYCY).
    pub fn len(&self) -> usize {
        self.lock().sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().sessions.is_empty()
    }

    pub fn get(&self, id: &str) -> Result<Arc<Session>, TerminalError> {
        self.lock()
            .sessions
            .iter()
            .find(|s| s.spec.id == id)
            .cloned()
            .ok_or_else(|| TerminalError::new("unknown_terminal"))
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        let mut inner = self.lock();
        let index = inner.sessions.iter().position(|s| s.spec.id == id)?;
        Some(inner.sessions.remove(index))
    }

    /// Every session, in creation order.
    pub fn all(&self) -> Vec<Arc<Session>> {
        self.lock().sessions.clone()
    }

    /// Takes every session out, for a project close (TRS-FR-OTLH).
    pub fn drain(&self) -> Vec<Arc<Session>> {
        std::mem::take(&mut self.lock().sessions)
    }

    pub fn summaries(&self) -> Vec<TerminalSummary> {
        self.all().iter().map(|s| s.summary()).collect()
    }

    pub fn for_target(&self, target: &TargetRef) -> Vec<Arc<Session>> {
        self.all().into_iter().filter(|s| &s.spec.target == target).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(path: &str) -> NewSession {
        NewSession {
            target: TargetRef::Worktree { path: path.into() },
            target_name: "repo".into(),
            branch: None,
            kind: TerminalKind::HostShell,
            vendor: None,
            runs_in: RunsIn::Host,
            busy_run_id: None,
            initial_state: TerminalState::Running,
        }
    }

    // TRS-FR-AYCY: the 17th session is refused.
    #[test]
    fn refuses_past_sixteen_sessions() {
        let registry = Registry::default();
        for i in 0..MAX_SESSIONS {
            registry.add(format!("t{i}"), String::new(), host("/r")).unwrap();
        }
        let err = registry.add("t16".into(), String::new(), host("/r")).err().unwrap();
        assert_eq!(err.code, "terminal_limit_reached");
        assert_eq!(registry.all().len(), MAX_SESSIONS);
    }

    // TRS-FR-AYCY: an ended session still counts, and a removed one does not.
    #[test]
    fn ended_sessions_count_and_removed_ones_do_not() {
        let registry = Registry::default();
        for i in 0..MAX_SESSIONS {
            let s = registry.add(format!("t{i}"), String::new(), host("/r")).unwrap();
            s.process_ended(Some(0), EndReason::ProcessExited);
        }
        assert!(registry.add("x".into(), String::new(), host("/r")).is_err());
        registry.remove("t3");
        assert!(registry.add("x".into(), String::new(), host("/r")).is_ok());
    }

    // TRS-FR-LRYH: labels take the lowest free number for their name.
    #[test]
    fn labels_take_the_lowest_free_number() {
        let registry = Registry::default();
        let a = registry.add("a".into(), String::new(), host("/r")).unwrap();
        let b = registry.add("b".into(), String::new(), host("/r")).unwrap();
        assert_eq!(a.spec.label, "Host shell 1");
        assert_eq!(b.spec.label, "Host shell 2");
        registry.remove("a");
        let c = registry.add("c".into(), String::new(), host("/r")).unwrap();
        assert_eq!(c.spec.label, "Host shell 1");
        let mut agent = host("/r");
        agent.kind = TerminalKind::AgentCli;
        agent.vendor = Some(CliVendor::Codex);
        let d = registry.add("d".into(), String::new(), agent).unwrap();
        assert_eq!(d.spec.label, "Codex 1");
    }

    // TRS-FR-LRYH, TRS-FR-BTMT: the number looks at the current labels, so a
    // renamed session frees its number and a label taken by a rename is skipped.
    #[test]
    fn numbers_follow_the_current_labels() {
        let registry = Registry::default();
        let a = registry.add("a".into(), String::new(), host("/r")).unwrap();
        a.set_label("build".into());
        let b = registry.add("b".into(), String::new(), host("/r")).unwrap();
        assert_eq!(b.label(), "Host shell 1");
        b.set_label("Host shell 2".into());
        let c = registry.add("c".into(), String::new(), host("/r")).unwrap();
        assert_eq!(c.label(), "Host shell 1");
        let d = registry.add("d".into(), String::new(), host("/r")).unwrap();
        assert_eq!(d.label(), "Host shell 3");
    }

    // TRS-FR-TEHT, TRS-FR-OTLH: a drain takes every session out.
    #[test]
    fn drain_empties_the_registry() {
        let registry = Registry::default();
        registry.add("a".into(), String::new(), host("/r")).unwrap();
        assert_eq!(registry.drain().len(), 1);
        assert!(registry.summaries().is_empty());
    }

    // TRS-FR-RKAW: sessions are found by target.
    #[test]
    fn finds_sessions_by_target() {
        let registry = Registry::default();
        registry.add("a".into(), String::new(), host("/one")).unwrap();
        registry.add("b".into(), String::new(), host("/two")).unwrap();
        let found = registry.for_target(&TargetRef::Worktree { path: "/two".into() });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].spec.id, "b");
    }

    // TRS-FR-IEKU, TRS-FR-TFAN, TRS-FR-AYCY: a restored session keeps its id
    // and label, an id held already is skipped, and the limit counts it.
    #[test]
    fn restores_with_the_given_id_and_label() {
        let registry = Registry::default();
        let restored = registry
            .restore("t9".into(), "Host shell 4".into(), "2026".into(), host("/r"))
            .unwrap()
            .unwrap();
        assert_eq!(restored.spec.label, "Host shell 4");
        assert!(registry.contains("t9"));
        assert!(registry
            .restore("t9".into(), "Host shell 4".into(), "2026".into(), host("/r"))
            .unwrap()
            .is_none());
        for i in 1..MAX_SESSIONS {
            registry.add(format!("t{i}x"), String::new(), host("/r")).unwrap();
        }
        assert_eq!(registry.len(), MAX_SESSIONS);
        let err =
            registry.restore("u".into(), "x".into(), String::new(), host("/r")).err().unwrap();
        assert_eq!(err.code, "terminal_limit_reached");
    }

    // TRS-FR-YYER: an unknown id is a typed error.
    #[test]
    fn an_unknown_id_is_a_typed_error() {
        let registry = Registry::default();
        assert_eq!(registry.get("nope").err().unwrap().code, "unknown_terminal");
    }
}
