//! The launch environment rules that need no launch to test (EAC-FR-OWPP).
//!
//! Split out of `launch.rs` for size alone.

use std::collections::BTreeMap;

use super::*;

/// EAC-FR-OWPP: remove from the launch environment every variable whose name an
/// executor-set variable already uses, and return those names.
///
/// Pure so the rule is testable without a launch: the executor-set names that
/// can collide with an author's entry are the repository variables, which exist
/// only for a directory inside a Git repository.
pub(super) fn drop_executor_set_variables(
    env_names: &mut Vec<String>,
    env: &mut BTreeMap<String, SecretString>,
    masked_env: &mut std::collections::BTreeSet<String>,
    env_literals: &[(String, String)],
) -> Vec<String> {
    let dropped: Vec<String> = env_names
        .iter()
        .filter(|name| env_literals.iter().any(|(set, _)| set == *name))
        .cloned()
        .collect();
    env_names.retain(|name| !dropped.contains(name));
    for name in &dropped {
        env.remove(name);
        masked_env.remove(name);
    }
    dropped
}
