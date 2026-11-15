//! Per-project state (`PWN-project-windows.md`, PWN-FR-ISOL).
//!
//! Each open project owns one scope. A scope holds one value of each type that
//! serves one project: the project root, the file watch, the indexes, the runs.
//! Two scopes never share a value, so work in one project cannot reach another.
//!
//! A command names its project with the `projectId` field of the invoke
//! payload. The [`Scoped`] argument type reads it and gives the value of that
//! scope. Background code gets its value with [`ScopeAccess::scoped`] and the
//! project id that it captured when it started.
//!
//! A call that carries no `projectId` uses the [`DEFAULT_SCOPE`]. The Project
//! picker window uses it: it holds no project.

pub mod context;
pub mod events;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

use tauri::ipc::{CommandArg, CommandItem, InvokeBody, InvokeError};
use tauri::{Manager, Runtime, State};

/// The key of the scope for a call that names no project.
pub const DEFAULT_SCOPE: &str = "";

/// The name of the invoke payload field that holds the project id.
pub const PROJECT_ID_FIELD: &str = "projectId";

type Slot = Arc<dyn Any + Send + Sync>;

/// All the scopes of the application.
#[derive(Default)]
pub struct ProjectScopes {
    scopes: Mutex<HashMap<String, HashMap<TypeId, Slot>>>,
}

impl ProjectScopes {
    /// The value of type `T` in one scope. The value is made with `T::default()`
    /// the first time that the scope asks for it.
    pub fn get<T: Default + Send + Sync + 'static>(&self, project_id: &str) -> Arc<T> {
        let mut scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        let slots = scopes.entry(project_id.to_string()).or_default();
        let slot = slots
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(T::default()) as Slot)
            .clone();
        match slot.downcast::<T>() {
            Ok(value) => value,
            // The slot is keyed by `TypeId::of::<T>()`, so the type is always `T`.
            Err(_) => unreachable!("scope slot has the type of its key"),
        }
    }

    /// The value of type `T` in one scope, if the scope holds one.
    pub fn peek<T: Send + Sync + 'static>(&self, project_id: &str) -> Option<Arc<T>> {
        let scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        let slot = scopes.get(project_id)?.get(&TypeId::of::<T>())?.clone();
        slot.downcast::<T>().ok()
    }

    /// Put a value in one scope. It replaces the value that the scope holds.
    pub fn set<T: Send + Sync + 'static>(&self, project_id: &str, value: T) {
        let mut scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        scopes
            .entry(project_id.to_string())
            .or_default()
            .insert(TypeId::of::<T>(), Arc::new(value) as Slot);
    }

    /// Drop a whole scope. Every value in it is released when the last holder
    /// lets go.
    pub fn remove(&self, project_id: &str) {
        let mut scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        scopes.remove(project_id);
    }

    /// The keys of every scope, the default scope included.
    pub fn scope_ids(&self) -> Vec<String> {
        let scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        let mut ids: Vec<String> = scopes.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// The project ids that have a scope. The default scope is not listed.
    pub fn project_ids(&self) -> Vec<String> {
        let scopes = self.scopes.lock().unwrap_or_else(|e| e.into_inner());
        let mut ids: Vec<String> =
            scopes.keys().filter(|k| k.as_str() != DEFAULT_SCOPE).cloned().collect();
        ids.sort();
        ids
    }
}

/// A value of the scope of one project.
pub enum Scoped<'r, T: Send + Sync + 'static> {
    /// A value of the scope of the project with this id.
    Owned(Arc<T>, String),
    /// A value that the application manages for every project. Only a test uses
    /// it: a test manages the value on a mock application.
    Managed(State<'r, T>),
}

impl<T: Send + Sync + 'static> Clone for Scoped<'_, T> {
    fn clone(&self) -> Self {
        match self {
            Scoped::Owned(value, id) => Scoped::Owned(value.clone(), id.clone()),
            Scoped::Managed(state) => Scoped::Managed(state.clone()),
        }
    }
}

impl<T: Send + Sync + 'static> Scoped<'_, T> {
    /// The value itself, as `State::inner` gives it.
    pub fn inner(&self) -> &T {
        self
    }
}

impl<T: Send + Sync + 'static> Deref for Scoped<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match self {
            Scoped::Owned(value, _) => value,
            Scoped::Managed(state) => state.inner(),
        }
    }
}

impl<'r, T: Send + Sync + 'static> From<State<'r, T>> for Scoped<'r, T> {
    fn from(state: State<'r, T>) -> Self {
        Scoped::Managed(state)
    }
}

impl<T: Send + Sync + 'static> Scoped<'_, T> {
    /// The id of the scope that holds the value. A value that the application
    /// manages for every project belongs to the default scope.
    pub fn project_id(&self) -> &str {
        match self {
            Scoped::Owned(_, id) => id,
            Scoped::Managed(_) => DEFAULT_SCOPE,
        }
    }

    /// The value as a shared handle that a background task can keep.
    pub fn shared(&self) -> Option<Arc<T>> {
        match self {
            Scoped::Owned(value, _) => Some(value.clone()),
            Scoped::Managed(_) => None,
        }
    }
}

/// The project id of a command call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectKey(pub String);

impl ProjectKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn project_id_of(body: &InvokeBody) -> String {
    match body {
        InvokeBody::Json(value) => value
            .get(PROJECT_ID_FIELD)
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_SCOPE)
            .to_string(),
        InvokeBody::Raw(_) => DEFAULT_SCOPE.to_string(),
    }
}

impl<'de, R: Runtime> CommandArg<'de, R> for ProjectKey {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        Ok(ProjectKey(project_id_of(command.message.payload())))
    }
}

impl<'r, 'de: 'r, R: Runtime, T: Default + Send + Sync + 'static> CommandArg<'de, R>
    for Scoped<'r, T>
{
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        let project_id = project_id_of(command.message.payload());
        let state = command.message.state_ref();
        if let Some(scopes) = state.try_get::<ProjectScopes>() {
            return Ok(Scoped::Owned(scopes.get::<T>(&project_id), project_id));
        }
        state.try_get::<T>().map(Scoped::Managed).ok_or_else(|| {
            InvokeError::from(format!(
                "state not managed for field `{}` on command `{}`",
                command.key, command.name
            ))
        })
    }
}

/// The scope lookup for background code.
pub trait ScopeAccess<R: Runtime> {
    /// The value of type `T` in the scope of one project.
    fn scoped<T: Default + Send + Sync + 'static>(&self, project_id: &str) -> Scoped<'_, T>;
}

/// The store of one project for code that has no command call.
pub trait ProjectStores<R: Runtime> {
    /// The value of type `T` of the project of the running code (PWN-FR-CTXT).
    /// Code that runs for no project gets the one open project's value. With two
    /// projects open and no context it gets the value of the default scope,
    /// which holds nothing of any project.
    fn project_store<T: Default + Send + Sync + 'static>(&self) -> Scoped<'_, T>;

    /// [`ProjectStores::project_store`], or `None` where the application holds
    /// no such value.
    fn try_project_store<T: Default + Send + Sync + 'static>(&self) -> Option<Scoped<'_, T>>;
}

impl<R: Runtime, M: Manager<R>> ProjectStores<R> for M {
    fn project_store<T: Default + Send + Sync + 'static>(&self) -> Scoped<'_, T> {
        let Some(scopes) = self.try_state::<ProjectScopes>() else {
            return Scoped::Managed(self.state::<T>());
        };
        let id = context::current()
            .or_else(|| sole_open_project_id(&scopes))
            .unwrap_or_else(|| DEFAULT_SCOPE.to_string());
        Scoped::Owned(scopes.get::<T>(&id), id)
    }

    fn try_project_store<T: Default + Send + Sync + 'static>(&self) -> Option<Scoped<'_, T>> {
        if self.try_state::<ProjectScopes>().is_none() {
            return self.try_state::<T>().map(Scoped::Managed);
        }
        Some(self.project_store::<T>())
    }
}

/// Run a routine of the whole application once for each project that has a
/// scope, in the context of that project (PWN-FR-CTXT). A timer or the quit
/// uses it: such a routine has no window of its own, so without this it would
/// act on no project, or on the project of one window only. Where the
/// application holds no project scope, the routine runs once as it is.
pub fn for_each_project<R: Runtime, M: Manager<R>>(app: &M, mut work: impl FnMut()) {
    let ids =
        app.try_state::<ProjectScopes>().map(|scopes| scopes.project_ids()).unwrap_or_default();
    if ids.is_empty() {
        work();
        return;
    }
    for id in ids {
        context::run_in(&id, &mut work);
    }
}

/// Whether a project other than `own_id` is open. `own_id` is the project of the
/// running code; `None` counts every open project as another. The process-wide
/// diagnostic buffer is cleared only when no other project is open
/// (`LGC-FR-15`).
pub fn other_project_open<R: Runtime, M: Manager<R>>(app: &M, own_id: Option<&str>) -> bool {
    let Some(scopes) = app.try_state::<ProjectScopes>() else {
        return false;
    };
    scopes.project_ids().into_iter().any(|id| {
        Some(id.as_str()) != own_id
            && scopes.peek::<crate::project::ProjectState>(&id).is_some_and(|s| s.root().is_some())
    })
}

/// The id of the one scope whose project is open, if exactly one is.
fn sole_open_project_id(scopes: &ProjectScopes) -> Option<String> {
    let mut open = scopes.scope_ids().into_iter().filter(|id| {
        scopes.peek::<crate::project::ProjectState>(id).is_some_and(|s| s.root().is_some())
    });
    let first = open.next()?;
    open.next().is_none().then_some(first)
}

impl<R: Runtime, M: Manager<R>> ScopeAccess<R> for M {
    fn scoped<T: Default + Send + Sync + 'static>(&self, project_id: &str) -> Scoped<'_, T> {
        if let Some(scopes) = self.try_state::<ProjectScopes>() {
            return Scoped::Owned(scopes.get::<T>(project_id), project_id.to_string());
        }
        Scoped::Managed(self.state::<T>())
    }
}

#[cfg(test)]
mod isolation_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter(Mutex<u32>);

    #[test]
    fn scopes_hold_separate_values() {
        let scopes = ProjectScopes::default();
        *scopes.get::<Counter>("a").0.lock().unwrap() = 5;
        assert_eq!(*scopes.get::<Counter>("a").0.lock().unwrap(), 5);
        assert_eq!(*scopes.get::<Counter>("b").0.lock().unwrap(), 0);
    }

    #[test]
    fn remove_drops_a_scope() {
        let scopes = ProjectScopes::default();
        *scopes.get::<Counter>("a").0.lock().unwrap() = 5;
        scopes.remove("a");
        assert_eq!(*scopes.get::<Counter>("a").0.lock().unwrap(), 0);
    }

    #[test]
    fn set_replaces_the_value() {
        let scopes = ProjectScopes::default();
        scopes.set("a", Counter(Mutex::new(9)));
        assert_eq!(*scopes.get::<Counter>("a").0.lock().unwrap(), 9);
    }

    #[test]
    fn project_ids_skip_the_default_scope() {
        let scopes = ProjectScopes::default();
        scopes.get::<Counter>(DEFAULT_SCOPE);
        scopes.get::<Counter>("b");
        scopes.get::<Counter>("a");
        assert_eq!(scopes.project_ids(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn payload_without_project_id_uses_the_default_scope() {
        let body = InvokeBody::Json(serde_json::json!({ "x": 1 }));
        assert_eq!(project_id_of(&body), DEFAULT_SCOPE);
        let body = InvokeBody::Json(serde_json::json!({ "projectId": "p1" }));
        assert_eq!(project_id_of(&body), "p1");
    }

    mod open_project {
        use super::super::*;
        use crate::project::{OpenProject, ProjectState};
        use tauri::test::mock_app;

        fn app_with_scopes() -> tauri::App<tauri::test::MockRuntime> {
            let app = mock_app();
            app.manage(ProjectScopes::default());
            app
        }

        #[test]
        fn no_project_open_gives_nothing() {
            let app = app_with_scopes();
            assert!(app.handle().open_project_state().is_none());
            assert!(app.handle().open_project_state_or_default().root().is_none());
        }

        #[test]
        fn one_open_project_is_found_in_its_scope() {
            let app = app_with_scopes();
            let dir = tempfile::tempdir().unwrap();
            app.state::<ProjectScopes>()
                .get::<ProjectState>("p1")
                .set_root(dir.path().to_path_buf());
            let found = app.handle().open_project_state().expect("one open project");
            assert_eq!(found.root().as_deref(), Some(dir.path()));
        }

        #[test]
        fn a_project_in_the_default_scope_counts_as_open() {
            let app = app_with_scopes();
            let dir = tempfile::tempdir().unwrap();
            app.state::<ProjectScopes>()
                .get::<ProjectState>(DEFAULT_SCOPE)
                .set_root(dir.path().to_path_buf());
            assert!(app.handle().open_project_state().is_some());
        }

        #[test]
        fn two_open_projects_give_nothing() {
            let app = app_with_scopes();
            let one = tempfile::tempdir().unwrap();
            let two = tempfile::tempdir().unwrap();
            let scopes = app.state::<ProjectScopes>();
            scopes.get::<ProjectState>("p1").set_root(one.path().to_path_buf());
            scopes.get::<ProjectState>("p2").set_root(two.path().to_path_buf());
            assert!(app.handle().open_project_state().is_none());
            assert!(app.handle().open_project_state_or_default().root().is_none());
        }

        #[test]
        fn a_closed_scope_leaves_the_other_project_alone() {
            let app = app_with_scopes();
            let one = tempfile::tempdir().unwrap();
            let two = tempfile::tempdir().unwrap();
            let scopes = app.state::<ProjectScopes>();
            scopes.get::<ProjectState>("p1").set_root(one.path().to_path_buf());
            scopes.get::<ProjectState>("p2").set_root(two.path().to_path_buf());
            scopes.remove("p1");
            let found = app.handle().open_project_state().expect("p2 is the sole project");
            assert_eq!(found.root().as_deref(), Some(two.path()));
        }
    }
}
