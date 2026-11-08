//! The earlier agent enrolment that a settings file may still hold.

use super::PersistedState;

/// GSS-FR-31: clear the earlier agent enrolment from every project slot.
/// Returns whether anything was cleared, which is what decides if the store is
/// written.
pub(super) fn drop_legacy_enrolment(state: &mut PersistedState) -> bool {
    let mut found = false;
    let slots = state.projects.values_mut().chain(std::iter::once(&mut state.default_project));
    for slot in slots {
        if !slot.legacy_enrolled_agents.is_empty() {
            slot.legacy_enrolled_agents.clear();
            found = true;
        }
    }
    found
}
