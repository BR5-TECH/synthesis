//! EAC-FR-UDQN, EAC-FR-LJXE: the one test of the Docker-in-Docker acceptance
//! test. It starts Postgres through Testcontainers on the Docker daemon that
//! the agent container reaches, connects to it, and runs `SELECT 1`.
//!
//! Testcontainers for Rust does not read `TESTCONTAINERS_HOST_OVERRIDE`, which
//! the Testcontainers libraries of other languages read. The test reads it
//! itself, so with the host gateway toggle on (EAC-FR-DKRS) it reaches
//! Postgres through `host.docker.internal`, as such a library does.

use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::SyncRunner;
use testcontainers_modules::testcontainers::Container;

const HOST_OVERRIDE: &str = "TESTCONTAINERS_HOST_OVERRIDE";

/// The port Docker published for Postgres. Docker can report the binding a
/// moment after the container starts, so the test asks again, for at most 30
/// seconds, until the binding shows.
fn published_port(node: &Container<Postgres>) -> u16 {
    let mut last = None;
    for _ in 0..120 {
        match node.get_host_port_ipv4(5432) {
            Ok(port) => return port,
            Err(error) => last = Some(error),
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    // What Docker reports for the container, so a failure explains itself.
    let ports = node.ports();
    let running = node.is_running();
    panic!("the published Postgres port: {last:?}; ports {ports:?}; running {running:?}");
}

#[test]
fn postgres_answers_select_one() {
    let node = Postgres::default().start().expect("Testcontainers starts Postgres");
    let host = match std::env::var(HOST_OVERRIDE) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => node.get_host().expect("the host of the container").to_string(),
    };
    let port = published_port(&node);
    // The acceptance test reads this line to see which host was used.
    println!("uat-testcontainers-host={host}");
    let url = format!("host={host} port={port} user=postgres password=postgres dbname=postgres");
    let mut client = postgres::Client::connect(&url, postgres::NoTls).expect("connect to Postgres");
    let row = client.query_one("SELECT 1", &[]).expect("SELECT 1");
    assert_eq!(row.get::<_, i32>(0), 1);
}
