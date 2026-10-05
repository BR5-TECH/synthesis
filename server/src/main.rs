//! The Synthesis backend microservice.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Requirements: BMS-FR-03, BMS-FR-07, BMS-FR-08, BMS-FR-09, BMS-FR-10.
//!
//! One standalone service, built on Axum over Tokio. It answers the
//! unauthenticated endpoint `GET /v1/health`, which reports the version the
//! binary was built from, and the authenticated routes of
//! `specifications/server/SAS-server-application-service.md` and
//! `specifications/server/SRB-server-relay-boundary.md`. The binary holds the
//! process entry point alone; every layer lives in the library (BMS-FR-03).

use std::ffi::OsString;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::process::ExitCode;

use tokio::net::TcpListener;
use tokio::sync::oneshot;

use synthesis_server::config::{self, ServerConfig};
use synthesis_server::identity_config::{self, IdentityConfig};
use synthesis_server::router;
use synthesis_server::shutdown::{self, DrainOutcome, Signals};
use synthesis_server::{build_state, version};

/// The name every diagnostic starts with.
const PROGRAM: &str = "synthesis-server";

/// The exit status of a configuration failure (BMS-FR-07).
const EXIT_CONFIGURATION: u8 = 2;
/// The exit status of a failure to start the service (BMS-FR-08).
///
/// It differs from `EXIT_CONFIGURATION`, so an operator tells a bind failure
/// from a setting that was refused.
const EXIT_RUNTIME: u8 = 1;

/// Starts the HTTP service and returns only after the service has stopped
/// (BMS-FR-03).
fn main() -> ExitCode {
    // `args_os` and `var_os`, not `args` and `var`: the first of those panics on
    // an argument that is not valid Unicode, and the second reports such a
    // variable as absent. Both outcomes are wrong here, so the operating-system
    // strings are carried into the resolver, which refuses them as the startup
    // failure BMS-FR-07 describes.
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();

    // The settings are resolved before the runtime is built, so a value that is
    // refused exits with no socket bound and no request served (BMS-FR-07).
    let settings = match config::resolve(&arguments, |name: &str| std::env::var_os(name)) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{PROGRAM}: {error}");
            return ExitCode::from(EXIT_CONFIGURATION);
        }
    };

    // SAS-FR-QJWD, SAS-FR-VKTP: the administrator identifier and the static
    // token are read before the listener is bound, and a value that is missing
    // or malformed exits with the configuration status of BMS-FR-07.
    let identity = match identity_config::resolve(|name: &str| std::env::var_os(name)) {
        Ok(identity) => identity,
        Err(error) => {
            eprintln!("{PROGRAM}: {error}");
            return ExitCode::from(EXIT_CONFIGURATION);
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{PROGRAM}: the asynchronous runtime did not start: {error}");
            return ExitCode::from(EXIT_RUNTIME);
        }
    };

    runtime.block_on(serve(settings, identity))
}

/// Binds the listener, serves the router, and drains on a termination signal.
async fn serve(settings: ServerConfig, identity: IdentityConfig) -> ExitCode {
    // SAS-FR-DTXV: the store is built and the administrator user is reconciled
    // before the listener is bound, so no request is served against a store
    // that holds no administrator.
    let state = match build_state(identity, version::BUILD_VERSION) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{PROGRAM}: the administrator user was not reconciled: {error}");
            return ExitCode::from(EXIT_CONFIGURATION);
        }
    };

    // The handlers are installed before the listener is bound, so a signal that
    // arrives during startup is not lost (BMS-FR-10).
    let mut signals = match Signals::install() {
        Ok(signals) => signals,
        Err(error) => {
            eprintln!("{PROGRAM}: the termination signal handlers did not install: {error}");
            return ExitCode::from(EXIT_RUNTIME);
        }
    };

    let address = SocketAddr::new(settings.host, settings.port);
    let listener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("{}", bind_failure_message(address, &error));
            return ExitCode::from(EXIT_RUNTIME);
        }
    };

    let bound = listener.local_addr().unwrap_or(address);

    // BMS-FR-09: one record, written after the listener is bound and before the
    // first request is served. It carries no credential and no user content,
    // because the service holds neither.
    write_record(&listening_record(bound, version::BUILD_VERSION));

    let (drain_sender, drain_receiver) = oneshot::channel::<()>();

    // RSN-FR-CLGY: the open WebSocket connections are closed with
    // `relay_restarted` when the drain starts, so a client distinguishes a
    // stopping relay from a protocol failure of its own.
    let hub = std::sync::Arc::clone(&state.http.hub);

    let mut server = tokio::spawn(async move {
        axum::serve(listener, router::build_router(state))
            .with_graceful_shutdown(async move {
                let _ = drain_receiver.await;
            })
            .await
    });

    let signal_name = tokio::select! {
        name = signals.recv() => name,
        result = &mut server => {
            eprintln!("{}", stopped_early_message(result));
            return ExitCode::from(EXIT_RUNTIME);
        }
    };

    let closed = hub
        .close_all(synthesis_server::adapters::relay::session::ReasonCode::RelayRestarted.as_str());
    if closed > 0 {
        // RSN-FR-CLGY: the operator reads from the log alone that the open
        // relay connections were closed, and with which reason.
        write_record(
            &serde_json::json!({
                "event": "relay_shutdown",
                "reason": "relay_restarted",
                "connections": closed,
            })
            .to_string(),
        );
    }

    // The service stops accepting new connections here. The requests already in
    // flight keep their connection until they complete or the drain period
    // elapses (BMS-FR-10).
    write_record(&draining_record(signal_name));
    let _ = drain_sender.send(());

    let outcome = shutdown::await_drain(
        async {
            let _ = (&mut server).await;
        },
        async {
            let _ = signals.recv().await;
        },
        shutdown::DRAIN_PERIOD,
    )
    .await;

    write_record(&stopped_record(outcome));
    ExitCode::SUCCESS
}

/// Writes one record to standard output.
///
/// A write that fails is dropped rather than raised. A collector that closed the
/// pipe must not turn a clean shutdown into a crash, and `println!` would panic
/// on exactly that (BMS-FR-10).
fn write_record(record: &str) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{record}");
    let _ = stdout.flush();
}

/// The record the process writes after the listener is bound (BMS-FR-09).
///
/// It names the bound address, the bound port, and the resolved build version,
/// so an operator reads from the log alone which build runs and where it
/// listens.
fn listening_record(address: SocketAddr, build_version: &str) -> String {
    serde_json::json!({
        "event": "listening",
        "address": address.ip().to_string(),
        "port": address.port(),
        "version": build_version,
    })
    .to_string()
}

/// The record the process writes when a termination signal starts the drain.
fn draining_record(signal_name: &str) -> String {
    serde_json::json!({
        "event": "draining",
        "signal": signal_name,
        "drain_seconds": shutdown::DRAIN_PERIOD.as_secs(),
    })
    .to_string()
}

/// The record the process writes just before it exits 0.
fn stopped_record(outcome: DrainOutcome) -> String {
    serde_json::json!({
        "event": "stopped",
        "outcome": outcome.as_str(),
    })
    .to_string()
}

/// The diagnostic of a failure to bind (BMS-FR-08).
///
/// It names the address and the port that could not be bound, so an operator
/// tells "the address is in use" from "the process may not have it" without
/// reading the code.
fn bind_failure_message(address: SocketAddr, error: &io::Error) -> String {
    format!(
        "{PROGRAM}: the address {} and the port {} could not be bound: {error}",
        address.ip(),
        address.port()
    )
}

/// The diagnostic of an HTTP service that stopped with no signal.
fn stopped_early_message(result: Result<io::Result<()>, tokio::task::JoinError>) -> String {
    let cause = match result {
        Ok(Ok(())) => "it stopped accepting connections".to_string(),
        Ok(Err(error)) => error.to_string(),
        Err(error) => error.to_string(),
    };
    format!("{PROGRAM}: the HTTP service stopped before a termination signal arrived: {cause}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::net::{IpAddr, Ipv4Addr};

    fn address(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }

    // BMS-FR-09, BMS-FR-11: the first record names the bound address, the bound port, and
    // the version that GET /v1/health then reports.
    #[test]
    fn the_listening_record_names_the_address_the_port_and_the_version() {
        let record = listening_record(address(9099), "v1.4.0");
        let parsed: serde_json::Value =
            serde_json::from_str(&record).expect("the record is a JSON document");

        assert_eq!(parsed["event"], "listening");
        assert_eq!(parsed["address"], "127.0.0.1");
        assert_eq!(parsed["port"], 9099);
        assert_eq!(parsed["version"], "v1.4.0");
    }

    #[test]
    fn the_listening_record_is_one_line() {
        let record = listening_record(address(8080), "v1.4.0");
        assert!(!record.contains('\n'), "{record}");
    }

    // BMS-FR-09: the record reports the same version the health endpoint does.
    #[test]
    fn the_listening_record_reports_the_compiled_in_version() {
        let record = listening_record(address(8080), version::BUILD_VERSION);
        let parsed: serde_json::Value =
            serde_json::from_str(&record).expect("the record is a JSON document");
        assert_eq!(parsed["version"], version::BUILD_VERSION);
    }

    // BMS-FR-08: the diagnostic names the address and the port.
    #[test]
    fn the_bind_diagnostic_names_the_address_and_the_port() {
        let error = io::Error::new(ErrorKind::AddrInUse, "address already in use");
        let message = bind_failure_message(address(9099), &error);

        assert!(message.contains("127.0.0.1"), "{message}");
        assert!(message.contains("9099"), "{message}");
        assert!(message.starts_with(PROGRAM), "{message}");
    }

    // BMS-FR-08: a bind failure and a configuration failure exit differently.
    #[test]
    fn a_bind_failure_and_a_configuration_failure_use_different_statuses() {
        assert_eq!(EXIT_CONFIGURATION, 2);
        assert_eq!(EXIT_RUNTIME, 1);
        assert_ne!(EXIT_CONFIGURATION, EXIT_RUNTIME);
    }

    #[test]
    fn the_drain_records_are_one_line_each() {
        assert!(!draining_record("SIGTERM").contains('\n'));
        assert!(!stopped_record(DrainOutcome::Drained).contains('\n'));
    }

    #[test]
    fn the_draining_record_names_the_signal_and_the_period() {
        let parsed: serde_json::Value = serde_json::from_str(&draining_record("SIGTERM"))
            .expect("the record is a JSON document");
        assert_eq!(parsed["signal"], "SIGTERM");
        assert_eq!(parsed["drain_seconds"], 10);
    }

    #[test]
    fn the_early_stop_diagnostic_explains_the_cause() {
        let message = stopped_early_message(Ok(Ok(())));
        assert!(
            message.contains("stopped accepting connections"),
            "{message}"
        );
    }
}
