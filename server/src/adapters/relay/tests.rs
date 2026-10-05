//! The tests of the relay routing state.
//!
//! Specification: `specifications/server/SRB-server-relay-boundary.md`.

use std::sync::Arc;

use crate::adapters::relay::port::{
    ClientConnection, ExposedProject, RelayApi, WorkerRegistration,
};
use crate::adapters::relay::session::{HandleState, InvalidReason, ReasonCode};
use crate::adapters::relay::state::RelayRegistry;
use crate::application::logging::{CaptureSink, LogSink};
use crate::domain::clock::{Clock, SystemClock};
use crate::domain::ids::{IdSource, RandomIds};

fn relay() -> RelayRegistry {
    relay_with_logs().0
}

/// A relay and the sink its records are read back from (SRB-FR-IZAB).
fn relay_with_logs() -> (RelayRegistry, Arc<CaptureSink>) {
    let logs = Arc::new(CaptureSink::new());
    let relay = RelayRegistry::new(
        Arc::new(SystemClock) as Arc<dyn Clock>,
        Arc::new(RandomIds) as Arc<dyn IdSource>,
        Arc::clone(&logs) as Arc<dyn LogSink>,
    );
    (relay, logs)
}

/// A handle the IDE accepted, which is what the client endpoint routes
/// (`RSN-remote-session.md` RSN-FR-HSNA).
fn accepted_handle(
    relay: &RelayRegistry,
    registration_id: &str,
    instance_id: &str,
) -> ClientConnection {
    let client = relay
        .register_client(registration_id, instance_id)
        .expect("a registration");
    relay
        .resolve_registration(&client.handle, instance_id, true)
        .expect("the IDE accepted the registration")
}

fn projects(names: &[&str]) -> Vec<ExposedProject> {
    names
        .iter()
        .map(|name| ExposedProject {
            project_id: (*name).to_string(),
            display_name: format!("The {name} project"),
        })
        .collect()
}

// SRB-FR-YCFA, SRB-FR-DKPM: a route holds the instance, the connection, the
// exposed projects, and the selected project.
#[test]
fn a_worker_route_holds_what_the_specification_names() {
    let relay = relay();
    let registration = relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the worker is registered");

    assert!(matches!(registration, WorkerRegistration::Fresh(_)));
    let route = registration.route();
    assert_eq!(route.instance_id, "instance-a");
    assert_eq!(route.connection_id, "connection-1");
    assert_eq!(route.projects.len(), 2);
    assert_eq!(route.selected_project_id, None);
    assert_eq!(route.attached_clients, 0);
}

// SRB-FR-LGQB: one instance holds one route, and a second connection replaces
// the first and leaves a notice for it.
#[test]
fn a_second_connection_replaces_the_first_and_leaves_a_notice() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the first connection");
    let registration = relay
        .register_worker("instance-a", "connection-2", projects(&["p1"]))
        .expect("the second connection");

    match registration {
        WorkerRegistration::Replaced {
            route,
            previous_connection_id,
            ..
        } => {
            assert_eq!(route.connection_id, "connection-2");
            assert_eq!(previous_connection_id, "connection-1");
        }
        other => panic!("the second connection replaced nothing: {other:?}"),
    }

    assert_eq!(relay.workers().len(), 1);
    let notice = relay
        .take_replacement_notice("connection-1")
        .expect("the replaced connection reads a notice");
    assert_eq!(notice.instance_id, "instance-a");
    assert_eq!(notice.replaced_connection_id, "connection-1");
    // The notice is read once.
    assert!(relay.take_replacement_notice("connection-1").is_none());

    // The older connection no longer owns the route, so it drops nothing.
    assert!(relay.drop_worker("instance-a", "connection-1").is_err());
}

// SRB-FR-NIUT: the newer announcement replaces the whole exposed set.
#[test]
fn an_announcement_replaces_the_whole_exposed_set() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the worker is registered");
    relay
        .select_project("instance-a", "p2")
        .expect("the project is selected");

    let route = relay
        .announce_projects("instance-a", "connection-1", projects(&["p1", "p3"]), None)
        .expect("the announcement");
    let exposed: Vec<&str> = route
        .projects
        .iter()
        .map(|project| project.project_id.as_str())
        .collect();
    assert_eq!(exposed, vec!["p1", "p3"]);
    // The selected project left the exposed set, so it is selected no longer.
    assert_eq!(route.selected_project_id, None);
}

// SRB-FR-FVJD: a target the worker does not expose is refused, and the
// selected project does not change.
#[test]
fn an_unexposed_target_is_refused_and_changes_nothing() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the worker is registered");
    relay
        .select_project("instance-a", "p1")
        .expect("the project is selected");

    let error = relay
        .select_project("instance-a", "p9")
        .expect_err("an unexposed target is refused");
    assert_eq!(error.code(), "not_found");
    assert_eq!(
        relay
            .worker("instance-a")
            .expect("the route stands")
            .selected_project_id,
        Some("p1".to_string())
    );
}

// SRB-FR-OPBZ, SRB-FR-RQOD: two workers hold two selected projects, and the
// clients attached to each observe their own worker's project.
#[test]
fn two_workers_hold_two_selected_projects_and_their_clients_observe_each() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the first worker");
    relay
        .register_worker("instance-b", "connection-2", projects(&["p3"]))
        .expect("the second worker");
    relay
        .select_project("instance-a", "p2")
        .expect("a selection");
    relay
        .select_project("instance-b", "p3")
        .expect("a selection");

    // Three handles: two on one worker and one on the other.
    let first = accepted_handle(&relay, "reg-1", "instance-a");
    let second = accepted_handle(&relay, "reg-2", "instance-a");
    let third = accepted_handle(&relay, "reg-3", "instance-b");
    assert_ne!(first.handle, second.handle);

    relay
        .attach_client(&first.handle, "instance-a")
        .expect("an attachment");
    relay
        .attach_client(&second.handle, "instance-a")
        .expect("an attachment");
    relay
        .attach_client(&third.handle, "instance-b")
        .expect("an attachment");

    assert_eq!(
        relay
            .observe_selected_project(&first.handle)
            .expect("an observation"),
        Some("p2".to_string())
    );
    assert_eq!(
        relay
            .observe_selected_project(&third.handle)
            .expect("an observation"),
        Some("p3".to_string())
    );

    // A selection on one worker reaches its own clients alone.
    relay
        .select_project("instance-a", "p1")
        .expect("a selection");
    assert_eq!(
        relay
            .observe_selected_project(&second.handle)
            .expect("an observation"),
        Some("p1".to_string())
    );
    assert_eq!(
        relay
            .observe_selected_project(&third.handle)
            .expect("an observation"),
        Some("p3".to_string())
    );

    assert_eq!(
        relay
            .worker("instance-a")
            .expect("a route")
            .attached_clients,
        2
    );
    assert_eq!(
        relay
            .worker("instance-b")
            .expect("a route")
            .attached_clients,
        1
    );
}

// SRB-FR-MTHQ: an attachment to an instance that holds no route is refused.
#[test]
fn an_attachment_to_an_unknown_worker_is_refused() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");

    // RSN-FR-UDCM: the handle is bound to one worker, and it never moves.
    let error = relay
        .attach_client(&client.handle, "instance-none")
        .expect_err("the attachment is refused");
    assert_eq!(error, ReasonCode::HandleNotFound);
    assert_eq!(
        relay
            .client(&client.handle)
            .expect("the connection")
            .instance_id,
        None
    );

    // RSN-FR-BUXE: a registration whose target holds no route is refused before
    // a handle is assigned.
    assert_eq!(
        relay
            .register_client("reg-2", "instance-none")
            .expect_err("no route holds the target"),
        ReasonCode::WorkerNotFound
    );
}

// RSN-FR-FKRE, RSN-FR-HSNA: a pending handle is not eligible for the client
// endpoint, and a rejected one never becomes eligible.
#[test]
fn a_handle_is_eligible_only_after_the_ide_accepts_it() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");

    let pending = relay
        .register_client("reg-1", "instance-a")
        .expect("a registration");
    assert_eq!(pending.state, HandleState::Pending);
    assert_eq!(
        relay
            .attach_client(&pending.handle, "instance-a")
            .expect_err("a pending handle attaches to nothing"),
        ReasonCode::HandleNotFound
    );

    let rejected = relay
        .register_client("reg-2", "instance-a")
        .expect("a registration");
    relay
        .resolve_registration(&rejected.handle, "instance-a", false)
        .expect("the IDE rejected the registration");
    assert_eq!(
        relay
            .attach_client(&rejected.handle, "instance-a")
            .expect_err("a rejected handle attaches to nothing"),
        ReasonCode::HandleNotFound
    );

    // RSN-FR-GPLZ: one registration takes exactly one terminal result.
    assert_eq!(
        relay
            .resolve_registration(&rejected.handle, "instance-a", true)
            .expect_err("a second result is refused"),
        ReasonCode::RegistrationNotFound
    );
}

// RSN-FR-DWLS, RSN-FR-KVBO, RSN-FR-ADXL: the IDE invalidates a handle for the
// reason it names, and every later frame is refused with that same reason.
#[test]
fn an_invalidated_handle_is_refused_with_the_reason_it_carries() {
    for (reason, expected) in [
        (InvalidReason::Revoked, ReasonCode::HandleRevoked),
        (InvalidReason::Expired, ReasonCode::HandleExpired),
        (InvalidReason::Reset, ReasonCode::HandleReset),
    ] {
        let relay = relay();
        relay
            .register_worker("instance-a", "connection-1", projects(&["p1"]))
            .expect("the worker");
        let client = accepted_handle(&relay, "reg-1", "instance-a");
        relay
            .attach_client(&client.handle, "instance-a")
            .expect("an attachment");

        relay
            .invalidate_handle(&client.handle, Some("instance-a"), reason)
            .expect("the handle is invalidated");

        let held = relay.client(&client.handle).expect("the record is kept");
        assert_eq!(held.state, HandleState::Invalid(reason));
        assert_eq!(held.instance_id, None);
        assert!(!held.authenticated);
        assert_eq!(
            relay
                .attach_client(&client.handle, "instance-a")
                .expect_err("the handle is refused"),
            expected
        );
        assert_eq!(
            relay
                .authenticate_client(&client.handle, "instance-a")
                .expect_err("the handle is refused"),
            expected
        );
    }
}

// SRB-FR-HZLE, SRB-FR-PWNK: a broadcast reaches the authenticated connections
// of that route alone.
#[test]
fn a_broadcast_reaches_the_authenticated_clients_of_one_route() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the first worker");
    relay
        .register_worker("instance-b", "connection-2", projects(&["p1"]))
        .expect("the second worker");

    let authenticated = accepted_handle(&relay, "reg-1", "instance-a");
    let pending = accepted_handle(&relay, "reg-2", "instance-a");
    let elsewhere = accepted_handle(&relay, "reg-3", "instance-b");
    relay
        .attach_client(&authenticated.handle, "instance-a")
        .expect("an attachment");
    relay
        .attach_client(&pending.handle, "instance-a")
        .expect("an attachment");
    relay
        .attach_client(&elsewhere.handle, "instance-b")
        .expect("an attachment");
    relay
        .authenticate_client(&authenticated.handle, "instance-a")
        .expect("the worker authenticated the client");
    relay
        .authenticate_client(&elsewhere.handle, "instance-b")
        .expect("the worker authenticated the client");

    assert_eq!(
        relay
            .broadcast_targets("instance-a", "connection-1")
            .expect("the targets"),
        vec![authenticated.handle.clone()]
    );
    assert_eq!(
        relay
            .broadcast_targets("instance-b", "connection-2")
            .expect("the targets"),
        vec![elsewhere.handle]
    );
}

// SRB-FR-AVTK: a reset invalidates the handle, and the handle is never reused.
#[test]
fn a_reset_invalidates_the_handle_for_good() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");

    relay.reset_client(&client.handle).expect("the reset");
    // RSN-FR-ADXL: the record is kept, so a later frame is refused with the
    // reason of the reset rather than as an unknown handle.
    assert_eq!(
        relay
            .attach_client(&client.handle, "instance-a")
            .expect_err("the handle is refused"),
        ReasonCode::HandleReset
    );
    assert_eq!(
        relay
            .observe_selected_project(&client.handle)
            .expect_err("the handle is refused"),
        ReasonCode::HandleReset
    );

    // Every later registration takes another handle.
    for index in 0..32 {
        let next = relay
            .register_client(&format!("reg-{index}"), "instance-a")
            .expect("a registration");
        assert_ne!(next.handle, client.handle);
    }
}

// SRB-FR-GJSP: dropping a route detaches the connections it held.
#[test]
fn dropping_a_route_detaches_its_clients() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");
    relay
        .authenticate_client(&client.handle, "instance-a")
        .expect("the authentication");

    let detached = relay
        .drop_worker("instance-a", "connection-1")
        .expect("the route is dropped");
    assert_eq!(detached, vec![client.handle.clone()]);
    assert!(relay.workers().is_empty());

    let connection = relay.client(&client.handle).expect("the connection stands");
    assert_eq!(connection.instance_id, None);
    assert!(!connection.authenticated);
    assert_eq!(
        relay
            .observe_selected_project(&client.handle)
            .expect("an observation"),
        None
    );
}

// SRB-FR-WHRO, SRB-FR-ZUPL: a new relay holds no route and no handle, which is
// what a restart leaves.
#[test]
fn a_new_relay_holds_no_route_and_no_handle() {
    let first = relay();
    first
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&first, "reg-1", "instance-a");

    let second = relay();
    assert!(second.workers().is_empty());
    assert!(second.worker("instance-a").is_err());
    assert!(second.client(&client.handle).is_err());
}

// SRB-FR-VUCM: the state is safe to drive from many connections at once.
#[test]
fn the_state_is_driven_from_many_threads() {
    let relay = Arc::new(relay());
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let relay = Arc::clone(&relay);
            std::thread::spawn(move || {
                let instance = format!("instance-{index}");
                relay
                    .register_worker(&instance, &format!("connection-{index}"), projects(&["p1"]))
                    .expect("the worker");
                for round in 0..25 {
                    let client =
                        accepted_handle(&relay, &format!("reg-{index}-{round}"), &instance);
                    relay
                        .attach_client(&client.handle, &instance)
                        .expect("an attachment");
                    relay
                        .authenticate_client(&client.handle, &instance)
                        .expect("the authentication");
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("the thread completes");
    }

    assert_eq!(relay.workers().len(), 8);
    for route in relay.workers() {
        assert_eq!(route.attached_clients, 25);
        assert_eq!(
            relay
                .broadcast_targets(&route.instance_id, &route.connection_id)
                .expect("the targets")
                .len(),
            25
        );
    }
}

// SRB-FR-EASV: the relay routes by the project identifier alone. Two projects
// may carry one display name, and neither is reached by it.
#[test]
fn the_display_name_is_never_a_routing_key() {
    let relay = relay();
    let exposed = vec![
        ExposedProject {
            project_id: "p1".to_string(),
            display_name: "One name".to_string(),
        },
        ExposedProject {
            project_id: "p2".to_string(),
            display_name: "One name".to_string(),
        },
    ];
    relay
        .register_worker("instance-a", "connection-1", exposed)
        .expect("the worker");

    assert!(relay.select_project("instance-a", "One name").is_err());
    relay
        .select_project("instance-a", "p2")
        .expect("the identifier selects the project");
}

// SRB-FR-IZAB: every operation of the port writes one record, and the record
// names the operation, the connection identifier, and the `instance_id`.
#[test]
fn a_worker_operation_writes_a_record_of_the_connection_and_the_instance() {
    let (relay, logs) = relay_with_logs();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    relay
        .select_project("instance-a", "p1")
        .expect("the project is selected");

    let records = logs.records_of("relay");
    assert_eq!(records.len(), 2);

    assert_eq!(records[0]["operation"], "register_worker");
    assert_eq!(records[0]["connection_id"], "connection-1");
    assert_eq!(records[0]["instance_id"], "instance-a");
    assert_eq!(records[0]["outcome"], "ok");

    assert_eq!(records[1]["operation"], "select_project");
    assert_eq!(records[1]["connection_id"], "connection-1");
    assert_eq!(records[1]["instance_id"], "instance-a");
}

// SRB-FR-IZAB: a refused operation is recorded as well, with the code of the
// refusal as its outcome.
#[test]
fn a_refused_operation_is_recorded_with_the_code_of_the_refusal() {
    let (relay, logs) = relay_with_logs();
    assert!(relay.select_project("instance-none", "p1").is_err());

    let records = logs.records_of("relay");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["operation"], "select_project");
    assert_eq!(records[0]["instance_id"], "instance-none");
    assert_eq!(records[0]["outcome"], "not_found");
}

// SRB-FR-IZAB, SRB-FR-XSAG: a client connection reaches a record by its
// connection identifier. The handle reaches no record.
#[test]
fn a_client_operation_names_the_connection_and_never_the_handle() {
    let (relay, logs) = relay_with_logs();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("the attachment");
    relay
        .authenticate_client(&client.handle, "instance-a")
        .expect("the client is authenticated");
    relay.reset_client(&client.handle).expect("the reset");

    let text = logs.text();
    assert!(
        !text.contains(&client.handle),
        "a record named the handle: {text}"
    );
    assert_ne!(client.connection_id, client.handle);

    let records = logs.records_of("relay");
    for operation in [
        "register_client",
        "resolve_registration",
        "attach_client",
        "authenticate_client",
        "reset_client",
    ] {
        let record = records
            .iter()
            .find(|record| record["operation"] == operation)
            .unwrap_or_else(|| panic!("the operation {operation} wrote a record"));
        assert_eq!(
            record["connection_id"], client.connection_id,
            "the record of {operation} names the connection"
        );
    }
}

// SRB-FR-IZAB: a record holds the operation, the two identifiers, and the
// outcome alone. No project identifier, no display name, and no payload of any
// kind reaches it.
#[test]
fn a_record_holds_no_payload_and_no_project_content() {
    let (relay, logs) = relay_with_logs();
    relay
        .register_worker("instance-a", "connection-1", projects(&["secret-project"]))
        .expect("the worker");
    relay
        .announce_projects(
            "instance-a",
            "connection-1",
            projects(&["secret-project"]),
            None,
        )
        .expect("the announcement");

    let text = logs.text();
    assert!(!text.contains("secret-project"), "{text}");
    assert!(!text.contains("The secret-project project"), "{text}");

    for record in logs.records_of("relay") {
        let object = record.as_object().expect("a record is an object");
        let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "connection_id",
                "event",
                "instance_id",
                "operation",
                "outcome"
            ]
        );
    }
}

// RSN-FR-CQEF, RSN-FR-VTKA: the exposed set and the selection are applied
// together, and a selection the same announcement does not expose is refused
// without changing either.
#[test]
fn an_announcement_applies_its_set_and_its_selection_together() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the worker");

    let route = relay
        .announce_projects(
            "instance-a",
            "connection-1",
            projects(&["p1", "p2"]),
            Some("p2"),
        )
        .expect("the announcement");
    assert_eq!(route.selected_project_id, Some("p2".to_string()));

    let error = relay
        .announce_projects("instance-a", "connection-1", projects(&["p3"]), Some("p9"))
        .expect_err("a selection the announcement does not expose is refused");
    assert_eq!(error.code(), "not_found");

    // Neither half was applied.
    let route = relay.worker("instance-a").expect("the route stands");
    assert_eq!(route.selected_project_id, Some("p2".to_string()));
    assert_eq!(route.projects.len(), 2);
}

// RSN-FR-UDCM: a worker reaches no handle bound to another worker.
#[test]
fn a_worker_reaches_no_handle_of_another_worker() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the first worker");
    relay
        .register_worker("instance-b", "connection-2", projects(&["p1"]))
        .expect("the second worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");

    assert_eq!(
        relay
            .authenticate_client(&client.handle, "instance-b")
            .expect_err("another worker authenticates nothing"),
        ReasonCode::HandleNotFound
    );
    assert_eq!(
        relay
            .invalidate_handle(&client.handle, Some("instance-b"), InvalidReason::Revoked)
            .expect_err("another worker invalidates nothing"),
        ReasonCode::HandleNotFound
    );
    assert!(
        !relay
            .client(&client.handle)
            .expect("the connection stands")
            .authenticated
    );

    // Its own worker still reaches it.
    relay
        .authenticate_client(&client.handle, "instance-a")
        .expect("the owning worker authenticates its client");
}

// RSN-FR-JAVE: the attachment and the authentication belong to the handle, so a
// second connection observes the state the first one earned rather than
// clearing it. A handle attaching to another route starts unauthenticated.
#[test]
fn a_second_attachment_of_one_handle_keeps_what_the_handle_earned() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");
    relay
        .authenticate_client(&client.handle, "instance-a")
        .expect("the authentication");

    let again = relay
        .attach_client(&client.handle, "instance-a")
        .expect("a second attachment");
    assert!(again.authenticated);

    // A route the handle is not bound to is refused rather than attached
    // unauthenticated.
    relay
        .register_worker("instance-b", "connection-2", projects(&["p1"]))
        .expect("the second worker");
    assert_eq!(
        relay
            .attach_client(&client.handle, "instance-b")
            .expect_err("the handle is bound elsewhere"),
        ReasonCode::HandleNotFound
    );
}

// RSN-FR-YAOC: a registration the IDE never resolved leaves no handle behind,
// and the handle is still never assigned again (SRB-FR-XSAG).
#[test]
fn an_abandoned_registration_leaves_no_handle() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let pending = relay
        .register_client("reg-1", "instance-a")
        .expect("a registration");

    relay.drop_pending_registration(&pending.handle);
    assert_eq!(
        relay.peek_client(&pending.handle).expect_err("it is gone"),
        ReasonCode::HandleNotFound
    );

    // An accepted handle is not a pending registration, so it is left alone.
    let accepted = accepted_handle(&relay, "reg-2", "instance-a");
    relay.drop_pending_registration(&accepted.handle);
    assert!(relay.peek_client(&accepted.handle).is_ok());

    for index in 0..32 {
        let next = relay
            .register_client(&format!("reg-fresh-{index}"), "instance-a")
            .expect("a registration");
        assert_ne!(next.handle, pending.handle);
    }
}

// RSN-FR-ADXL: a reset is permanent, so a later invalidation never masks it.
#[test]
fn a_reset_is_never_masked_by_a_later_reason() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");

    relay.reset_client(&client.handle).expect("the reset");
    relay
        .invalidate_handle(&client.handle, Some("instance-a"), InvalidReason::Revoked)
        .expect("the invalidation is applied");

    assert_eq!(
        relay
            .attach_client(&client.handle, "instance-a")
            .expect_err("the handle is refused"),
        ReasonCode::HandleReset
    );
}

// SRB-FR-IZAB: the read the transport makes on every inbound frame writes no
// record, so a busy session does not evict the records a reader needs.
#[test]
fn peeking_at_a_handle_writes_no_record() {
    let (relay, logs) = relay_with_logs();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1"]))
        .expect("the worker");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    let before = logs.records_of("relay").len();

    for _ in 0..32 {
        let _ = relay.peek_client(&client.handle);
        let _ = relay.route_connection("instance-a");
    }
    assert_eq!(logs.records_of("relay").len(), before);
}

// SRB-FR-LGQB, RSN-FR-EHUT: a connection that no longer holds the route changes
// nothing, and reaches none of the successor's clients. The check is under the
// same lock as the write, so a replacement never loses a race with an older
// connection.
#[test]
fn a_replaced_connection_announces_nothing_and_broadcasts_to_nobody() {
    let relay = relay();
    relay
        .register_worker("instance-a", "connection-1", projects(&["p1", "p2"]))
        .expect("the first connection");
    let client = accepted_handle(&relay, "reg-1", "instance-a");
    relay
        .attach_client(&client.handle, "instance-a")
        .expect("an attachment");
    relay
        .authenticate_client(&client.handle, "instance-a")
        .expect("the authentication");

    relay
        .register_worker("instance-a", "connection-2", projects(&["p1", "p2"]))
        .expect("the replacement");
    relay
        .announce_projects(
            "instance-a",
            "connection-2",
            projects(&["p1", "p2"]),
            Some("p2"),
        )
        .expect("the replacement announces");

    // The replaced connection keeps writing, and changes nothing.
    assert!(relay
        .announce_projects("instance-a", "connection-1", projects(&["p9"]), Some("p9"))
        .is_err());
    assert_eq!(
        relay
            .broadcast_targets("instance-a", "connection-1")
            .expect_err("a replaced connection reaches nobody"),
        ReasonCode::WorkerNotFound
    );

    let route = relay.worker("instance-a").expect("the route stands");
    assert_eq!(route.connection_id, "connection-2");
    assert_eq!(route.selected_project_id, Some("p2".to_string()));
    let exposed: Vec<&str> = route
        .projects
        .iter()
        .map(|project| project.project_id.as_str())
        .collect();
    assert_eq!(exposed, vec!["p1", "p2"]);
}
