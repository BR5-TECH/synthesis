//! The in-memory relay routing state.
//!
//! Specification: `specifications/server/SRB-server-relay-boundary.md`.
//! Requirements: SRB-FR-ZUPL, SRB-FR-WHRO, SRB-FR-LGQB, SRB-FR-NIUT,
//! SRB-FR-OPBZ, SRB-FR-FVJD, SRB-FR-XSAG, SRB-FR-MTHQ, SRB-FR-RQOD,
//! SRB-FR-CBWU, SRB-FR-AVTK, SRB-FR-GJSP, SRB-FR-PWNK, SRB-FR-VUCM.
//!
//! One lock over one set of maps. Nothing here is written to disk, and a
//! restart drops every route and every handle.
//!
//! SRB-FR-IZAB: every operation of the port writes one record that names the
//! connection identifier, the `instance_id`, and the operation. A record holds
//! no token, no handle, no key, and no payload: the handle of a client
//! connection never reaches a record, and the connection is named there by the
//! separate identifier of `ClientConnection::connection_id`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::adapters::relay::port::{
    ClientConnection, ExposedProject, RelayApi, ReplacementNotice, WorkerRegistration, WorkerRoute,
};
use crate::adapters::relay::session::{HandleState, InvalidReason, ReasonCode};
use crate::application::logging::LogSink;
use crate::domain::clock::{Clock, Timestamp};
use crate::domain::error::DomainError;
use crate::domain::ids::IdSource;

/// The route of one worker, as the state holds it.
#[derive(Debug, Clone)]
struct Worker {
    connection_id: String,
    connected_at: Timestamp,
    projects: Vec<ExposedProject>,
    selected_project_id: Option<String>,
}

/// The state behind the lock.
#[derive(Debug, Default)]
struct RelayState {
    workers: HashMap<String, Worker>,
    clients: HashMap<String, ClientConnection>,
    /// Every handle the process has assigned, including the handles it has
    /// invalidated. A handle is never assigned twice (SRB-FR-XSAG).
    assigned_handles: HashSet<String>,
    notices: HashMap<String, ReplacementNotice>,
}

/// The relay routing state (SRB-FR-ZUPL).
pub struct RelayRegistry {
    state: Mutex<RelayState>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdSource>,
    logs: Arc<dyn LogSink>,
}

impl RelayRegistry {
    /// A relay that holds no route.
    pub fn new(clock: Arc<dyn Clock>, ids: Arc<dyn IdSource>, logs: Arc<dyn LogSink>) -> Self {
        RelayRegistry {
            state: Mutex::new(RelayState::default()),
            clock,
            ids,
            logs,
        }
    }

    /// Writes the record of one relay operation (SRB-FR-IZAB).
    ///
    /// The record is written after the operation, so its outcome names either
    /// `ok` or the code of the refusal. The lock is not held here: a record is
    /// written from the values the operation already read.
    fn record(
        &self,
        operation: &str,
        connection_id: Option<&str>,
        instance_id: Option<&str>,
        outcome: &str,
    ) {
        self.logs.write(serde_json::json!({
            "event": "relay",
            "operation": operation,
            "connection_id": connection_id,
            "instance_id": instance_id,
            "outcome": outcome,
        }));
    }

    /// Writes the record of an operation and reports its result unchanged.
    ///
    /// The identifiers reach the record from the result when it holds them, so
    /// one call covers the accepted path and the refused path alike.
    fn recorded_reason<T>(
        &self,
        operation: &str,
        connection_id: Option<&str>,
        instance_id: Option<&str>,
        result: Result<T, ReasonCode>,
    ) -> Result<T, ReasonCode> {
        let outcome = match &result {
            Ok(_) => "ok",
            Err(reason) => reason.as_str(),
        };
        self.record(operation, connection_id, instance_id, outcome);
        result
    }

    fn recorded<T>(
        &self,
        operation: &str,
        connection_id: Option<&str>,
        instance_id: Option<&str>,
        result: Result<T, DomainError>,
    ) -> Result<T, DomainError> {
        let outcome = match &result {
            Ok(_) => "ok",
            Err(error) => error.code(),
        };
        self.record(operation, connection_id, instance_id, outcome);
        result
    }

    /// The log identifier of the connection a handle names (SRB-FR-IZAB).
    ///
    /// A record names the connection, never the handle, so the handle is looked
    /// up here and left behind. An unknown handle names no connection.
    fn connection_id_of(&self, handle: &str) -> Option<String> {
        self.state
            .lock()
            .expect("the relay state is not poisoned")
            .clients
            .get(handle)
            .map(|client| client.connection_id.clone())
    }

    /// Refuses an identifier that is empty or too long.
    fn check_identifier(field: &str, value: &str) -> Result<String, DomainError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::invalid_field(field, "the value is empty"));
        }
        if trimmed.chars().count() > 200 {
            return Err(DomainError::invalid_field(
                field,
                "the value is longer than 200 characters",
            ));
        }
        Ok(trimmed.to_string())
    }

    /// Refuses an opaque routing field that is empty or too long
    /// (WSK-FR-QOZF).
    fn check_routing_field(value: &str) -> Result<String, ReasonCode> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 200 {
            return Err(ReasonCode::InvalidFrameShape);
        }
        Ok(trimmed.to_string())
    }

    /// The route as the port reports it, with its client count.
    fn route_of(state: &RelayState, instance_id: &str, worker: &Worker) -> WorkerRoute {
        WorkerRoute {
            instance_id: instance_id.to_string(),
            connection_id: worker.connection_id.clone(),
            connected_at: worker.connected_at.clone(),
            projects: worker.projects.clone(),
            selected_project_id: worker.selected_project_id.clone(),
            attached_clients: state
                .clients
                .values()
                .filter(|client| client.instance_id.as_deref() == Some(instance_id))
                .count(),
        }
    }
}

/// The operations of the port, with no record of their own.
///
/// The trait implementation below writes the record of SRB-FR-IZAB around each
/// of them, so the rules and the diagnostic stay apart: an operation here reads
/// and writes the state alone.
impl RelayRegistry {
    fn register_worker_inner(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
    ) -> Result<WorkerRegistration, DomainError> {
        let instance_id = Self::check_identifier("instance_id", instance_id)?;
        let connection_id = Self::check_identifier("connection_id", connection_id)?;
        let now = self.clock.now();

        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let previous = state.workers.get(&instance_id).cloned();

        state.workers.insert(
            instance_id.clone(),
            Worker {
                connection_id: connection_id.clone(),
                connected_at: now.clone(),
                projects,
                // SRB-FR-LGQB: the newer connection takes the route, and the
                // selected project of the older connection leaves with it.
                selected_project_id: None,
            },
        );

        // SRB-FR-GJSP: a replaced route detaches the clients that held it.
        let mut detached_handles = Vec::new();
        if previous.is_some() {
            for client in state.clients.values_mut() {
                if client.instance_id.as_deref() == Some(instance_id.as_str()) {
                    client.instance_id = None;
                    client.authenticated = false;
                    detached_handles.push(client.handle.clone());
                }
            }
            detached_handles.sort();
        }

        let worker = state
            .workers
            .get(&instance_id)
            .cloned()
            .expect("the route was just written");
        let route = Self::route_of(&state, &instance_id, &worker);

        match previous {
            None => Ok(WorkerRegistration::Fresh(route)),
            Some(previous) => {
                let notice = ReplacementNotice {
                    instance_id: instance_id.clone(),
                    replaced_connection_id: previous.connection_id.clone(),
                    replaced_at: now,
                };
                state.notices.insert(previous.connection_id.clone(), notice);
                Ok(WorkerRegistration::Replaced {
                    route,
                    previous_connection_id: previous.connection_id,
                    detached_handles,
                })
            }
        }
    }

    fn announce_projects_inner(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
        selected_project_id: Option<&str>,
    ) -> Result<WorkerRoute, DomainError> {
        // RSN-FR-VTKA: a selection that the same announcement does not expose
        // is refused before anything is written.
        if let Some(selected) = selected_project_id {
            if !projects
                .iter()
                .any(|project| project.project_id == selected)
            {
                return Err(DomainError::NotFound {
                    resource: "exposed project",
                });
            }
        }

        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let worker = state
            .workers
            .get_mut(instance_id)
            .ok_or(DomainError::NotFound {
                resource: "worker route",
            })?;

        // SRB-FR-LGQB, RSN-FR-EHUT: a connection that no longer holds the route
        // changes nothing. Checked under the same lock as the write, so a
        // replacement never loses a race with an older connection.
        if worker.connection_id != connection_id {
            return Err(DomainError::NotFound {
                resource: "worker route",
            });
        }

        // SRB-FR-NIUT: the announcement replaces the whole exposed set, and a
        // selected project that the newer set omits stops being selected. The
        // set and the selection are written under one lock, so no reader
        // observes one announcement's projects beside another's selection.
        let exposed: Vec<String> = projects
            .iter()
            .map(|project| project.project_id.clone())
            .collect();
        worker.projects = projects;
        worker.selected_project_id = match selected_project_id {
            Some(selected) => Some(selected.to_string()),
            None => worker
                .selected_project_id
                .clone()
                .filter(|selected| exposed.contains(selected)),
        };

        let worker = worker.clone();
        Ok(Self::route_of(&state, instance_id, &worker))
    }

    fn select_project_inner(
        &self,
        instance_id: &str,
        project_id: &str,
    ) -> Result<WorkerRoute, DomainError> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let worker = state
            .workers
            .get_mut(instance_id)
            .ok_or(DomainError::NotFound {
                resource: "worker route",
            })?;

        // SRB-FR-FVJD: a target that the worker does not expose is refused, and
        // the selected project does not change.
        if !worker
            .projects
            .iter()
            .any(|project| project.project_id == project_id)
        {
            return Err(DomainError::NotFound {
                resource: "exposed project",
            });
        }
        worker.selected_project_id = Some(project_id.to_string());

        let worker = worker.clone();
        Ok(Self::route_of(&state, instance_id, &worker))
    }

    fn drop_worker_inner(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, DomainError> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let worker = state
            .workers
            .get(instance_id)
            .ok_or(DomainError::NotFound {
                resource: "worker route",
            })?;

        // A connection that no longer holds the route drops nothing: it was
        // replaced, and the newer connection owns the route (SRB-FR-LGQB).
        if worker.connection_id != connection_id {
            return Err(DomainError::NotFound {
                resource: "worker route",
            });
        }
        state.workers.remove(instance_id);

        let mut detached = Vec::new();
        for client in state.clients.values_mut() {
            if client.instance_id.as_deref() == Some(instance_id) {
                client.instance_id = None;
                client.authenticated = false;
                detached.push(client.handle.clone());
            }
        }
        detached.sort();
        Ok(detached)
    }

    fn workers_inner(&self) -> Vec<WorkerRoute> {
        let state = self.state.lock().expect("the relay state is not poisoned");
        let mut routes: Vec<WorkerRoute> = state
            .workers
            .iter()
            .map(|(instance_id, worker)| Self::route_of(&state, instance_id, worker))
            .collect();
        routes.sort_by(|left, right| left.instance_id.cmp(&right.instance_id));
        routes
    }

    fn worker_inner(&self, instance_id: &str) -> Result<WorkerRoute, DomainError> {
        let state = self.state.lock().expect("the relay state is not poisoned");
        let worker = state
            .workers
            .get(instance_id)
            .ok_or(DomainError::NotFound {
                resource: "worker route",
            })?;
        Ok(Self::route_of(&state, instance_id, worker))
    }

    fn register_client_inner(
        &self,
        registration_id: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let registration_id = Self::check_routing_field(registration_id)?;
        let instance_id = Self::check_routing_field(instance_id)?;
        let now = self.clock.now();
        let mut state = self.state.lock().expect("the relay state is not poisoned");

        // RSN-FR-BUXE: a registration whose target holds no worker route is
        // refused before any handle is assigned.
        if !state.workers.contains_key(&instance_id) {
            return Err(ReasonCode::WorkerNotFound);
        }

        // SRB-FR-XSAG: the handle is unique among every handle the process has
        // assigned, including the handles it has invalidated.
        let mut handle = self.ids.next().to_string();
        while state.assigned_handles.contains(&handle) {
            handle = self.ids.next().to_string();
        }
        state.assigned_handles.insert(handle.clone());

        let connection = ClientConnection {
            handle: handle.clone(),
            // SRB-FR-IZAB: the identifier a record names. It is generated
            // beside the handle and never stands in for it.
            connection_id: self.ids.next().to_string(),
            registration_id,
            bound_instance_id: instance_id,
            // RSN-FR-FKRE: the handle is not eligible for the client endpoint
            // until the IDE accepts the registration.
            state: HandleState::Pending,
            instance_id: None,
            authenticated: false,
            registered_at: now,
        };
        state.clients.insert(handle, connection.clone());
        Ok(connection)
    }

    fn resolve_registration_inner(
        &self,
        handle: &str,
        instance_id: &str,
        accepted: bool,
    ) -> Result<ClientConnection, ReasonCode> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let client = state
            .clients
            .get_mut(handle)
            .ok_or(ReasonCode::HandleNotFound)?;

        // RSN-FR-UDCM: the result belongs to the worker the handle is bound to,
        // so one worker never resolves another worker's registration.
        if client.bound_instance_id != instance_id {
            return Err(ReasonCode::RegistrationNotFound);
        }
        // RSN-FR-GPLZ: exactly one terminal result resolves one registration.
        // A handle that is no longer pending was resolved already.
        if client.state != HandleState::Pending {
            return Err(ReasonCode::RegistrationNotFound);
        }
        client.state = if accepted {
            HandleState::Active
        } else {
            HandleState::Invalid(InvalidReason::Rejected)
        };
        Ok(client.clone())
    }

    fn attach_client_inner(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let holds_route = state.workers.contains_key(instance_id);
        let client = state
            .clients
            .get_mut(handle)
            .ok_or(ReasonCode::HandleNotFound)?;

        // RSN-FR-FKRE: a handle that is not active is refused with the reason
        // of its own state.
        if let Some(refusal) = client.state.refusal() {
            return Err(refusal);
        }
        // RSN-FR-UDCM: a handle presented against another worker is refused,
        // and it never moves to that worker.
        if client.bound_instance_id != instance_id {
            return Err(ReasonCode::HandleNotFound);
        }
        // SRB-FR-MTHQ: an attachment to an identifier that holds no active
        // route is refused.
        if !holds_route {
            return Err(ReasonCode::WorkerNotFound);
        }
        // RSN-FR-JAVE: the attachment and the authentication belong to the
        // handle, so a second connection presenting an authenticated handle
        // observes the state the first one earned rather than clearing it.
        // RSN-FR-GKZP: a handle that was not authenticated for this route is
        // attached unauthenticated.
        let held_the_route = client.instance_id.as_deref() == Some(instance_id);
        client.instance_id = Some(instance_id.to_string());
        client.authenticated = held_the_route && client.authenticated;
        Ok(client.clone())
    }

    fn authenticate_client_inner(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let client = state
            .clients
            .get_mut(handle)
            .ok_or(ReasonCode::HandleNotFound)?;
        if let Some(refusal) = client.state.refusal() {
            return Err(refusal);
        }
        // RSN-FR-UDCM, RSN-FR-FKRE: the worker that authenticates a client is
        // the one the handle is bound to and the one it is attached to. Another
        // worker never authenticates it.
        if client.bound_instance_id != instance_id {
            return Err(ReasonCode::HandleNotFound);
        }
        if client.instance_id.as_deref() != Some(instance_id) {
            return Err(ReasonCode::WorkerNotFound);
        }
        client.authenticated = true;
        Ok(client.clone())
    }

    fn observe_selected_project_inner(&self, handle: &str) -> Result<Option<String>, ReasonCode> {
        let state = self.state.lock().expect("the relay state is not poisoned");
        let client = state
            .clients
            .get(handle)
            .ok_or(ReasonCode::HandleNotFound)?;
        if let Some(refusal) = client.state.refusal() {
            return Err(refusal);
        }
        // SRB-FR-CBWU: the observation is the worker's selected project. The
        // connection holds no project of its own.
        Ok(client
            .instance_id
            .as_ref()
            .and_then(|instance_id| state.workers.get(instance_id))
            .and_then(|worker| worker.selected_project_id.clone()))
    }

    fn broadcast_targets_inner(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, ReasonCode> {
        let state = self.state.lock().expect("the relay state is not poisoned");
        // A connection that no longer holds the route reaches no client of it.
        if !state
            .workers
            .get(instance_id)
            .is_some_and(|worker| worker.connection_id == connection_id)
        {
            return Err(ReasonCode::WorkerNotFound);
        }
        let mut handles: Vec<String> = state
            .clients
            .values()
            .filter(|client| {
                client.authenticated && client.instance_id.as_deref() == Some(instance_id)
            })
            .map(|client| client.handle.clone())
            .collect();
        handles.sort();
        Ok(handles)
    }

    /// The connection the invalidation closed, so the record of the operation
    /// names it (SRB-FR-IZAB).
    ///
    /// RSN-FR-ADXL: the record is kept rather than removed, so a later frame
    /// that presents the handle is refused with the reason it was invalidated
    /// for rather than as a handle the relay never assigned.
    fn invalidate_handle_inner(
        &self,
        handle: &str,
        instance_id: Option<&str>,
        reason: InvalidReason,
    ) -> Result<ClientConnection, ReasonCode> {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let client = state
            .clients
            .get_mut(handle)
            .ok_or(ReasonCode::HandleNotFound)?;
        // RSN-FR-UDCM: a worker invalidates the handles bound to it alone.
        if let Some(instance_id) = instance_id {
            if client.bound_instance_id != instance_id {
                return Err(ReasonCode::HandleNotFound);
            }
        }
        // RSN-FR-ADXL: a reset is permanent, so a later reason never masks it.
        if client.state != HandleState::Invalid(InvalidReason::Reset) {
            client.state = HandleState::Invalid(reason);
        }
        client.instance_id = None;
        client.authenticated = false;
        Ok(client.clone())
    }

    /// RSN-FR-YAOC: a registration connection that closed before the IDE
    /// resolved it leaves no handle and no record behind.
    fn drop_pending_registration_inner(&self, handle: &str) -> bool {
        let mut state = self.state.lock().expect("the relay state is not poisoned");
        let pending = state
            .clients
            .get(handle)
            .is_some_and(|client| client.state == HandleState::Pending);
        if pending {
            // The handle itself stays in `assigned_handles`, so it is never
            // assigned again (SRB-FR-XSAG).
            state.clients.remove(handle);
        }
        pending
    }

    /// The connection that holds a route now, read without a record.
    fn route_connection_inner(&self, instance_id: &str) -> Option<String> {
        self.state
            .lock()
            .expect("the relay state is not poisoned")
            .workers
            .get(instance_id)
            .map(|worker| worker.connection_id.clone())
    }

    fn client_inner(&self, handle: &str) -> Result<ClientConnection, ReasonCode> {
        let state = self.state.lock().expect("the relay state is not poisoned");
        state
            .clients
            .get(handle)
            .cloned()
            .ok_or(ReasonCode::HandleNotFound)
    }

    fn take_replacement_notice_inner(&self, connection_id: &str) -> Option<ReplacementNotice> {
        self.state
            .lock()
            .expect("the relay state is not poisoned")
            .notices
            .remove(connection_id)
    }
}

/// The port, with the record of SRB-FR-IZAB around every operation.
///
/// A record names the operation, the connection identifier, and the
/// `instance_id` the operation worked on. It names no handle: a client
/// connection reaches a record by its `connection_id` alone, which the relay
/// looks up from the handle the caller presented. A refused operation is
/// recorded as well, with the code of the refusal as its outcome, so a failure
/// leaves a trail.
impl RelayApi for RelayRegistry {
    fn register_worker(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
    ) -> Result<WorkerRegistration, DomainError> {
        let result = self.register_worker_inner(instance_id, connection_id, projects);
        self.recorded(
            "register_worker",
            Some(connection_id),
            Some(instance_id),
            result,
        )
    }

    fn announce_projects(
        &self,
        instance_id: &str,
        connection_id: &str,
        projects: Vec<ExposedProject>,
        selected_project_id: Option<&str>,
    ) -> Result<WorkerRoute, DomainError> {
        let result =
            self.announce_projects_inner(instance_id, connection_id, projects, selected_project_id);
        self.recorded(
            "announce_projects",
            Some(connection_id),
            Some(instance_id),
            result,
        )
    }

    fn select_project(
        &self,
        instance_id: &str,
        project_id: &str,
    ) -> Result<WorkerRoute, DomainError> {
        let result = self.select_project_inner(instance_id, project_id);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|route| route.connection_id.clone());
        self.recorded(
            "select_project",
            connection_id.as_deref(),
            Some(instance_id),
            result,
        )
    }

    fn drop_worker(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, DomainError> {
        let result = self.drop_worker_inner(instance_id, connection_id);
        self.recorded(
            "drop_worker",
            Some(connection_id),
            Some(instance_id),
            result,
        )
    }

    fn workers(&self) -> Vec<WorkerRoute> {
        let routes = self.workers_inner();
        self.record("workers", None, None, "ok");
        routes
    }

    fn worker(&self, instance_id: &str) -> Result<WorkerRoute, DomainError> {
        let result = self.worker_inner(instance_id);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|route| route.connection_id.clone());
        self.recorded(
            "worker",
            connection_id.as_deref(),
            Some(instance_id),
            result,
        )
    }

    fn register_client(
        &self,
        registration_id: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let result = self.register_client_inner(registration_id, instance_id);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        self.recorded_reason(
            "register_client",
            connection_id.as_deref(),
            Some(instance_id),
            result,
        )
    }

    fn route_connection(&self, instance_id: &str) -> Option<String> {
        self.route_connection_inner(instance_id)
    }

    fn drop_pending_registration(&self, handle: &str) {
        let connection_id = self.connection_id_of(handle);
        let dropped = self.drop_pending_registration_inner(handle);
        // The outcome tells a reader whether the registration was abandoned or
        // had already been resolved (SRB-FR-IZAB).
        self.record(
            "drop_pending_registration",
            connection_id.as_deref(),
            None,
            if dropped { "ok" } else { "not_found" },
        );
    }

    fn resolve_registration(
        &self,
        handle: &str,
        instance_id: &str,
        accepted: bool,
    ) -> Result<ClientConnection, ReasonCode> {
        let result = self.resolve_registration_inner(handle, instance_id, accepted);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        let instance_id = result
            .as_ref()
            .ok()
            .map(|client| client.bound_instance_id.clone());
        self.recorded_reason(
            "resolve_registration",
            connection_id.as_deref(),
            instance_id.as_deref(),
            result,
        )
    }

    fn attach_client(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let result = self.attach_client_inner(handle, instance_id);
        // The connection reaches the record from the result, not from a second
        // read of the state: a reset between the two would leave the record
        // with no connection identifier at all.
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        self.recorded_reason(
            "attach_client",
            connection_id.as_deref(),
            Some(instance_id),
            result,
        )
    }

    fn authenticate_client(
        &self,
        handle: &str,
        instance_id: &str,
    ) -> Result<ClientConnection, ReasonCode> {
        let result = self.authenticate_client_inner(handle, instance_id);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        let instance_id = result
            .as_ref()
            .ok()
            .and_then(|client| client.instance_id.clone());
        self.recorded_reason(
            "authenticate_client",
            connection_id.as_deref(),
            instance_id.as_deref(),
            result,
        )
    }

    fn observe_selected_project(&self, handle: &str) -> Result<Option<String>, ReasonCode> {
        let result = self.observe_selected_project_inner(handle);
        let connection_id = self.connection_id_of(handle);
        self.recorded_reason(
            "observe_selected_project",
            connection_id.as_deref(),
            None,
            result,
        )
    }

    fn broadcast_targets(
        &self,
        instance_id: &str,
        connection_id: &str,
    ) -> Result<Vec<String>, ReasonCode> {
        let result = self.broadcast_targets_inner(instance_id, connection_id);
        self.recorded_reason(
            "broadcast_targets",
            Some(connection_id),
            Some(instance_id),
            result,
        )
    }

    fn invalidate_handle(
        &self,
        handle: &str,
        instance_id: Option<&str>,
        reason: InvalidReason,
    ) -> Result<ClientConnection, ReasonCode> {
        let result = self.invalidate_handle_inner(handle, instance_id, reason);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        self.recorded_reason(
            "invalidate_handle",
            connection_id.as_deref(),
            instance_id.as_deref(),
            result,
        )
    }

    fn reset_client(&self, handle: &str) -> Result<ClientConnection, ReasonCode> {
        let instance_id = self.client_inner(handle).ok().and_then(|c| c.instance_id);
        let result = self.invalidate_handle_inner(handle, None, InvalidReason::Reset);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        self.recorded_reason(
            "reset_client",
            connection_id.as_deref(),
            instance_id.as_deref(),
            result,
        )
    }

    fn peek_client(&self, handle: &str) -> Result<ClientConnection, ReasonCode> {
        self.client_inner(handle)
    }

    fn client(&self, handle: &str) -> Result<ClientConnection, ReasonCode> {
        let result = self.client_inner(handle);
        let connection_id = result
            .as_ref()
            .ok()
            .map(|client| client.connection_id.clone());
        let instance_id = result
            .as_ref()
            .ok()
            .and_then(|client| client.instance_id.clone());
        self.recorded_reason(
            "client",
            connection_id.as_deref(),
            instance_id.as_deref(),
            result,
        )
    }

    fn take_replacement_notice(&self, connection_id: &str) -> Option<ReplacementNotice> {
        let notice = self.take_replacement_notice_inner(connection_id);
        let instance_id = notice.as_ref().map(|notice| notice.instance_id.clone());
        self.record(
            "take_replacement_notice",
            Some(connection_id),
            instance_id.as_deref(),
            if notice.is_some() { "ok" } else { "not_found" },
        );
        notice
    }
}
