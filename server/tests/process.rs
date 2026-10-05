//! Process-level tests of the service.
//!
//! Specification: `specifications/server/BMS-backend-microservice.md`.
//! Scenarios: BMS-FR-03, BMS-FR-05, BMS-FR-07, BMS-FR-08, BMS-FR-09, BMS-FR-11,
//! BMS-FR-10, BMS-FR-12, BMS-FR-13, BMS-FR-27.
//!
//! The unit tests drive the router with no socket. These start the real binary,
//! so they cover what only a process shows: the bind, the exit statuses, the
//! first record on standard output, and the shutdown on a termination signal.
//!
//! No test needs the network: every socket is on the loopback interface, and
//! every port is one the operating system reported as free.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

/// The binary under test. Cargo builds it before the test runs.
const BINARY: &str = env!("CARGO_BIN_EXE_synthesis-server");

/// The administrator identifier a test starts the service with.
const DEFAULT_ADMIN_ID: &str = "1a4c5f6b-2d3e-4f50-9a8b-7c6d5e4f3a2b";
/// The static token a test presents to the service it started.
const DEFAULT_TOKEN: &str = "process-test-token-0123456789";

/// How long a test waits for the process to leave.
const EXIT_WAIT: Duration = Duration::from_secs(30);

/// How many times a test tries to start the service before it gives up.
const START_ATTEMPTS: usize = 8;

/// How long one attempt waits for the process to report that it is listening.
///
/// Without a deadline, a process that binds and then hangs blocks the read
/// forever and the whole lane dies at its own timeout with no diagnostic and no
/// stderr. With one, the attempt reports what the process said and retries.
const START_DEADLINE: Duration = Duration::from_secs(20);

/// The drain period the service compiles in (BMS-FR-10).
const DRAIN_PERIOD: Duration = Duration::from_secs(10);

/// How many times a drain test builds its starting state before it gives up.
const SETUP_ATTEMPTS: usize = 8;

/// Every port this test binary has handed out.
///
/// The tests run at the same time, and the operating system hands the same
/// ephemeral port to two callers once the first one has closed its probe. The
/// record makes each port unique inside the binary, so one test never binds the
/// port another test is about to use.
static HANDED_OUT: Mutex<Vec<u16>> = Mutex::new(Vec::new());

/// Reserves a port the operating system reports as free.
///
/// The probe listener is closed before the value is returned, so the service
/// binds the port itself.
fn free_port() -> u16 {
    let mut handed_out = HANDED_OUT.lock().expect("the record is not poisoned");

    for _ in 0..256 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a free port exists");
        let port = listener.local_addr().expect("the listener is bound").port();
        drop(listener);

        if !handed_out.contains(&port) {
            handed_out.push(port);
            return port;
        }
    }

    panic!("the operating system handed out no port this binary had not used already");
}

/// Binds and holds a port, so a test can prove the service refuses to bind it.
fn hold_port() -> (TcpListener, u16) {
    for _ in 0..START_ATTEMPTS {
        let port = free_port();
        if let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            return (listener, port);
        }
    }
    panic!("no probed port stayed free long enough to be held");
}

/// A running service, stopped when the test drops it.
struct Service {
    child: Child,
    port: u16,
    /// The first record the process wrote to standard output (BMS-FR-09).
    record: serde_json::Value,
    /// Kept open for the life of the test. Closing it would break the pipe the
    /// process writes its later records to, which is a property one test
    /// deliberately exercises.
    stdout: Option<BufReader<ChildStdout>>,
}

impl Service {
    /// Starts the binary on a free port and waits for its first record.
    ///
    /// `build` turns a port into the arguments and the variables the process is
    /// started with, so the caller decides which of the two supplies it.
    ///
    /// A port that was free when it was probed can be taken by any other
    /// process on the machine before the service binds it, so a bind failure is
    /// retried on a new port rather than reported as a defect of the service.
    fn start<F>(build: F) -> Self
    where
        F: Fn(u16) -> (Vec<String>, Vec<(String, String)>),
    {
        let mut last = String::from("none");

        for _ in 0..START_ATTEMPTS {
            let port = free_port();
            let (arguments, variables) = build(port);

            match Self::try_start(&arguments, &variables, port) {
                Ok(service) => return service,
                Err(diagnostic) => last = diagnostic,
            }
        }

        panic!("the service did not start in {START_ATTEMPTS} attempts; last diagnostic: {last}");
    }

    /// One attempt to start the binary.
    ///
    /// The first record is written after the listener is bound, so reading it is
    /// also how the test knows the service is ready. A process that writes none
    /// left before it bound, and its diagnostic is returned.
    fn try_start(
        arguments: &[String],
        variables: &[(String, String)],
        port: u16,
    ) -> Result<Self, String> {
        let mut child = spawn(arguments, variables);

        let stdout = child.stdout.take().expect("standard output is piped");

        // The read runs on its own thread, so a process that binds and then
        // hangs is killed by the deadline rather than blocking the test.
        let (sender, receiver) = mpsc::channel();
        let reading = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let read = reader.read_line(&mut line);
            let _ = sender.send(());
            (reader, line, read)
        });

        if receiver.recv_timeout(START_DEADLINE).is_err() {
            // Killing the child closes the pipe, which ends the reading thread.
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "the process wrote no record inside {START_DEADLINE:?}"
            ));
        }

        let (reader, line, read) = reading.join().expect("the reading thread does not panic");
        if let Err(error) = read {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("standard output was not readable: {error}"));
        }

        let record: serde_json::Value = match serde_json::from_str(line.trim()) {
            Ok(record) => record,
            Err(error) => {
                let _ = child.kill();
                let mut diagnostic = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut diagnostic);
                }
                let _ = child.wait();
                return Err(format!(
                    "the first record is not JSON: {line:?} ({error}); stderr: {}",
                    diagnostic.trim()
                ));
            }
        };

        Ok(Self {
            child,
            port,
            record,
            stdout: Some(reader),
        })
    }

    fn reader(&mut self) -> &mut BufReader<ChildStdout> {
        self.stdout.as_mut().expect("standard output is still open")
    }

    /// Closes the read end of the pipe the process writes its records to.
    fn close_standard_output(&mut self) {
        self.stdout = None;
    }

    /// The next record the process writes.
    ///
    /// Blocking, so it is called only where the process is known to write one.
    fn next_record(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.reader()
            .read_line(&mut line)
            .expect("standard output is readable");
        assert!(
            !line.trim().is_empty(),
            "the process wrote no further record"
        );

        serde_json::from_str(line.trim())
            .unwrap_or_else(|error| panic!("a record is not JSON: {line:?} ({error})"))
    }

    /// Every record the process wrote after the first one.
    ///
    /// Called after the process has left, so the read ends at the closed pipe
    /// rather than blocking. The records are small and the pipe holds them.
    fn remaining_records(&mut self) -> Vec<serde_json::Value> {
        let mut rest = String::new();
        self.reader()
            .read_to_string(&mut rest)
            .expect("standard output is readable");

        rest.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line.trim())
                    .unwrap_or_else(|error| panic!("a record is not JSON: {line:?} ({error})"))
            })
            .collect()
    }

    /// The `outcome` the last record reports (BMS-FR-10).
    fn drain_outcome(&mut self) -> String {
        let records = self.remaining_records();
        let last = records
            .last()
            .unwrap_or_else(|| panic!("the process wrote no record after the first"));
        assert_eq!(last["event"], "stopped", "the last record ends the process");
        last["outcome"]
            .as_str()
            .expect("the outcome is a string")
            .to_string()
    }

    /// True while the process has not left.
    fn is_running(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("the child is waitable")
            .is_none()
    }

    fn address(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    /// Sends a termination signal to the process.
    #[cfg(unix)]
    fn signal(&self, name: &str) {
        let status = Command::new("kill")
            .arg(format!("-{name}"))
            .arg(self.child.id().to_string())
            .status()
            .expect("kill runs on this platform");
        assert!(status.success(), "the {name} signal was not delivered");
    }

    /// Waits for the process to leave and reports its exit code.
    fn wait_for_exit(&mut self) -> i32 {
        let deadline = Instant::now() + EXIT_WAIT;
        loop {
            match self.child.try_wait().expect("the child is waitable") {
                Some(status) => return status.code().unwrap_or(-1),
                None if Instant::now() >= deadline => {
                    panic!("the process did not leave inside {EXIT_WAIT:?}")
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn<A, K, V>(arguments: &[A], variables: &[(K, V)]) -> Child
where
    A: AsRef<OsStr>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut command = Command::new(BINARY);
    command
        .args(arguments)
        .env_remove("SYNTHESIS_SERVER_HOST")
        .env_remove("SYNTHESIS_SERVER_PORT")
        // SAS-FR-QJWD: the administrator identifier and the static token are
        // needed to start. Every test that does not name them itself starts the
        // service with these, and a test of the startup failure names an empty
        // value, which the resolver reads as absent (SAS-FR-VKTP).
        .env("SYNTHESIS_SERVER_ADMIN_ID", DEFAULT_ADMIN_ID)
        .env("SYNTHESIS_SERVER_TOKEN", DEFAULT_TOKEN)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (name, value) in variables {
        command.env(name, value);
    }

    command.spawn().expect("the binary starts")
}

/// Starts the service on the loopback interface and on the given port.
fn local_bind(port: u16) -> (Vec<String>, Vec<(String, String)>) {
    (
        arguments(&["--host", "127.0.0.1", "--port", &port.to_string()]),
        vec![],
    )
}

fn arguments(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn variables(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

/// One HTTP response, as the tests read it.
struct Response {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    raw: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Sends one request and reads the whole answer.
///
/// The request asks for the connection to be closed, so reading to the end of
/// the stream is what ends the read. The client is written by hand, so the
/// tests add no HTTP client dependency to the crate.
fn request(address: SocketAddr, method: &str, path: &str) -> Response {
    let mut stream = TcpStream::connect(address).expect("the service accepts the connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the timeout is accepted");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");

    read_response(&mut stream)
}

/// One request that carries the bearer token (SAS-FR-LFCA).
fn request_with_token(address: SocketAddr, method: &str, path: &str, token: &str) -> Response {
    let mut stream = TcpStream::connect(address).expect("the service accepts the connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("the timeout is accepted");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");

    read_response(&mut stream)
}

/// Reads everything the service sends before it closes the connection.
///
/// A server that closes while unread request bytes are still in its receive
/// buffer makes the operating system reset the connection, and the client's
/// pending read then fails even though the whole answer already arrived. Bytes
/// that were read are therefore kept rather than discarded.
fn read_response(stream: &mut TcpStream) -> Response {
    let mut raw = Vec::new();
    let mut buffer = [0_u8; 4096];

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
            // Only a reset, and only after an answer arrived. A timeout must
            // not end the read: it would leave a truncated answer that the
            // caller cannot tell from a complete one.
            Err(error)
                if !raw.is_empty()
                    && matches!(
                        error.kind(),
                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                    ) =>
            {
                break
            }
            Err(error) => panic!("the answer is not readable: {error}"),
        }
    }

    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Response {
    let separator = b"\r\n\r\n";
    let head_end = raw
        .windows(separator.len())
        .position(|window| window == separator)
        .unwrap_or_else(|| {
            panic!(
                "the answer holds no header block: {:?}",
                String::from_utf8_lossy(raw)
            )
        });

    let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let body = raw[head_end + separator.len()..].to_vec();

    let mut lines = head.lines();
    let status_line = lines.next().expect("the answer holds a status line");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("the status line is malformed: {status_line}"));

    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();

    Response {
        status,
        headers,
        body,
        raw: raw.to_vec(),
    }
}

// BMS-FR-03, BMS-FR-05, BMS-FR-09, BMS-FR-11: with no host option and no host variable, the service
// binds the default address, and the first record names the address, the port,
// and the version that GET /v1/health then reports.
//
// The port is an operating-system-assigned free one rather than the default
// 8080, so two runs on the same machine never collide. That the default port is
// 8080 is asserted by the configuration unit tests and by the image guard.
#[test]
fn it_binds_the_default_host_and_answers_health() {
    let service = Service::start(|port| (arguments(&["--port", &port.to_string()]), vec![]));

    assert_eq!(service.record["event"], "listening");
    assert_eq!(service.record["address"], "0.0.0.0");
    assert_eq!(service.record["port"], service.port);

    let response = request(service.address(), "GET", "/v1/health");
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));

    let body: serde_json::Value =
        serde_json::from_slice(&response.body).expect("the body is a JSON document");
    let object = body.as_object().expect("the body is a JSON object");
    assert_eq!(object.len(), 2);
    assert_eq!(object["version"], service.record["version"]);
    // BMS-FR-JQZW: the running process advertises the two capabilities of V1.
    assert_eq!(
        object["capabilities"],
        serde_json::json!(["remote_session", "websocket"])
    );
}

// BMS-FR-05: the variables configure the bind when no option is given.
#[test]
fn the_environment_variables_configure_the_bind() {
    let service = Service::start(|port| {
        (
            vec![],
            variables(&[
                ("SYNTHESIS_SERVER_HOST", "127.0.0.1"),
                ("SYNTHESIS_SERVER_PORT", &port.to_string()),
            ]),
        )
    });

    assert_eq!(service.record["address"], "127.0.0.1");
    assert_eq!(service.record["port"], service.port);
    assert_eq!(request(service.address(), "GET", "/v1/health").status, 200);
}

// BMS-FR-05: the option wins over the variable, and the port the variable named
// is left free.
#[test]
fn the_option_wins_over_the_variable() {
    // The variable names a port the service must not use. It is held for the
    // life of the test, so the service could not bind it even if it tried, and
    // the port cannot be handed to another test either.
    let (_held, variable_port) = hold_port();

    let service = Service::start(|port| {
        (
            arguments(&["--port", &port.to_string()]),
            variables(&[
                ("SYNTHESIS_SERVER_HOST", "127.0.0.1"),
                ("SYNTHESIS_SERVER_PORT", &variable_port.to_string()),
            ]),
        )
    });

    assert_ne!(service.port, variable_port);
    assert_eq!(service.record["address"], "127.0.0.1");
    assert_eq!(service.record["port"], service.port);

    // The service answers on the option's port. Had the variable won, the
    // service would have failed to bind the port that is held and would never
    // have written a record at all.
    assert_eq!(request(service.address(), "GET", "/v1/health").status, 200);
}

// BMS-FR-07: a port that is not a port exits 2, and no socket was bound.
#[test]
fn an_invalid_port_exits_two_and_binds_no_socket() {
    let child = spawn(
        &arguments(&[]),
        &variables(&[
            ("SYNTHESIS_SERVER_HOST", "127.0.0.1"),
            ("SYNTHESIS_SERVER_PORT", "not-a-port"),
        ]),
    );

    let output = child.wait_with_output().expect("the process leaves");
    assert_eq!(output.status.code(), Some(2));

    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        diagnostic.lines().count(),
        1,
        "one diagnostic: {diagnostic}"
    );
    assert!(diagnostic.contains("port"), "{diagnostic}");
    assert!(diagnostic.contains("not-a-port"), "{diagnostic}");
    assert!(diagnostic.contains("1..=65535"), "{diagnostic}");
    // Empty standard output is the evidence that no listener was bound: the
    // process writes its one record after the bind and before the first
    // request, so its absence means the bind never happened.
    assert!(output.stdout.is_empty(), "no record was written");
}

// BMS-FR-07: a host that is not an IP address exits the same way.
#[test]
fn an_invalid_host_exits_two() {
    let child = spawn(&arguments(&["--host", "999.999.999.999"]), &variables(&[]));
    let output = child.wait_with_output().expect("the process leaves");

    assert_eq!(output.status.code(), Some(2));
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        diagnostic.lines().count(),
        1,
        "one diagnostic: {diagnostic}"
    );
    assert!(diagnostic.contains("host"), "{diagnostic}");
    assert!(diagnostic.contains("999.999.999.999"), "{diagnostic}");
    assert!(output.stdout.is_empty(), "no record was written");
}

// BMS-FR-07: an argument the operating system holds that is not valid Unicode
// exits 2 with one diagnostic, rather than panicking with status 101 as
// `std::env::args` would.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_unicode_exits_two() {
    use std::os::unix::ffi::OsStringExt;

    let child = spawn(
        &[std::ffi::OsString::from_vec(vec![0xff, 0xfe])],
        &variables(&[]),
    );
    let output = child.wait_with_output().expect("the process leaves");

    assert_eq!(output.status.code(), Some(2));
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        diagnostic.lines().count(),
        1,
        "one diagnostic: {diagnostic}"
    );
    assert!(diagnostic.contains("Unicode"), "{diagnostic}");
    assert!(!diagnostic.contains("panicked"), "{diagnostic}");
    assert!(output.stdout.is_empty(), "no record was written");
}

// BMS-FR-08: a port that is already held exits 1, which is not the 2 an invalid
// setting produces, and the diagnostic names the address and the port.
#[test]
fn a_port_that_is_already_held_exits_one() {
    let (holder, port) = hold_port();

    let port_text = port.to_string();
    let child = spawn(
        &arguments(&["--host", "127.0.0.1", "--port", &port_text]),
        &variables(&[]),
    );
    let output = child.wait_with_output().expect("the process leaves");

    assert_eq!(output.status.code(), Some(1));
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostic.contains("127.0.0.1"), "{diagnostic}");
    assert!(diagnostic.contains(&port_text), "{diagnostic}");

    drop(holder);
}

// BMS-FR-12: every answer is byte-identical.
#[test]
fn every_health_answer_is_byte_identical() {
    let service = Service::start(local_bind);

    let first = request(service.address(), "GET", "/v1/health").body;
    for _ in 0..10 {
        let response = request(service.address(), "GET", "/v1/health");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, first);
    }
}

// BMS-FR-13, BMS-FR-27: no other path and no other method is served.
#[test]
fn no_other_path_and_no_other_method_is_served() {
    let service = Service::start(local_bind);

    for path in ["/v1/status", "/health", "/v1/ws", "/"] {
        let response = request(service.address(), "GET", path);
        assert_eq!(response.status, 404, "path {path}");
        assert!(response.body.is_empty(), "path {path}");
    }

    for method in ["POST", "PUT", "DELETE"] {
        let response = request(service.address(), method, "/v1/health");
        assert_eq!(response.status, 405, "method {method}");
        assert!(response.body.is_empty(), "method {method}");
    }
}

/// Opens a connection and leaves its first request half-sent.
///
/// Three properties have to hold at once, and each rules out an easier shape:
///
/// - The connection must carry a request, not nothing. A graceful shutdown
///   closes an idle connection at once, so a connection that sent no bytes
///   would prove nothing about draining.
/// - The half-sent request must be the connection's **first**. Once a request
///   has completed on a connection, a half-sent next one leaves the connection
///   between requests, and a graceful shutdown closes it — so a completed
///   request cannot be used as a prologue on this connection.
/// - The service must have accepted the connection before the signal arrives.
///   A connection still in the listener's backlog when the listener closes is
///   dropped rather than drained.
///
/// The last one is settled without touching this connection: a second
/// connection is opened afterwards and answered in full. Connections are
/// accepted in the order they arrive, so an answer on the later connection is
/// proof that the earlier one was accepted first. That replaces a sleep, which
/// only ever makes the race less likely.
#[cfg(unix)]
fn connection_with_a_request_in_flight(address: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(address).expect("the service accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("the timeout is accepted");

    // Headers that have not ended: the service is reading a request that has
    // not arrived in full.
    stream
        .write_all(b"GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\n")
        .expect("the partial request is written");
    stream
        .flush()
        .expect("the partial request reaches the service");

    // The proof of acceptance, on its own connection.
    assert_eq!(
        request(address, "GET", "/v1/health").status,
        200,
        "the service did not answer a later connection, so the earlier one \
         cannot be assumed accepted"
    );

    stream
}

/// Runs a drain test against a service that holds a request in flight.
///
/// The starting state has one step the test cannot force: the service must have
/// **read** the half-sent request before the signal arrives, not merely accepted
/// the connection. Accept ordering proves the accept; nothing a client can send
/// proves the read, and under load the two come apart. A run where that did not
/// happen is not a failure of the requirement — it is a starting state that did
/// not build — and `body` reports it by returning `false`, which starts again on
/// a fresh service.
///
/// A run where it never builds fails loudly rather than passing on a weaker
/// property, so a service that genuinely stopped draining cannot hide here.
#[cfg(unix)]
fn with_a_request_in_flight<F>(body: F)
where
    F: Fn(&mut Service, &mut TcpStream) -> bool,
{
    for _ in 0..SETUP_ATTEMPTS {
        let mut service = Service::start(local_bind);
        let mut stream = connection_with_a_request_in_flight(service.address());

        if body(&mut service, &mut stream) {
            return;
        }
    }

    panic!("the service held no request in flight in {SETUP_ATTEMPTS} attempts");
}

/// Waits for the service to refuse a new connection, and reports whether it was
/// still draining when it did.
///
/// `false` means the process left before a refusal was seen, which is the
/// starting state of `with_a_request_in_flight` failing to build.
#[cfg(unix)]
fn refuses_new_connections_while_draining(service: &mut Service) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        if !service.is_running() {
            return false;
        }

        match TcpStream::connect(service.address()) {
            // Any failure to connect proves the claim, which is that the
            // service accepts no new connection. The usual one is a refusal,
            // but a machine under load can reset or time out the attempt
            // instead, and asserting the kind would fail on the weather rather
            // than on the requirement.
            Err(_) => {
                // Not accepted, and the process had not left when that was
                // observed — so it was still draining, which is the window the
                // requirement is about.
                return service.is_running();
            }
            Ok(_) => assert!(
                Instant::now() < deadline,
                "the service kept accepting connections after the signal"
            ),
        }
    }
}

// BMS-FR-10: on SIGTERM the request in flight completes and receives its answer,
// no new connection is accepted while the drain runs, and the process exits 0.
#[cfg(unix)]
#[test]
fn sigterm_drains_the_request_in_flight_and_refuses_new_connections() {
    with_a_request_in_flight(|service, stream| {
        service.signal("TERM");

        // The listener is closed the moment the signal is handled, so a new
        // connection is refused rather than accepted and left unanswered.
        if !refuses_new_connections_while_draining(service) {
            return false;
        }

        // The request in flight is ended and still receives its answer.
        stream
            .write_all(b"Connection: close\r\n\r\n")
            .expect("the rest of the request is written");
        stream.flush().expect("the rest reaches the service");

        let response = read_response(stream);
        assert_eq!(
            response.status,
            200,
            "the request in flight did not complete: {:?}",
            String::from_utf8_lossy(&response.raw)
        );

        assert_eq!(service.wait_for_exit(), 0);
        assert_eq!(service.drain_outcome(), "drained");
        true
    });
}

// BMS-FR-10: a request that is still in flight when the drain period elapses is
// dropped, and the process still exits 0. This is the requirement that stops a
// connection nobody is finishing from holding a container open until its
// runtime kills it, so it is worth the ten seconds it takes.
#[cfg(unix)]
#[test]
fn a_drain_that_does_not_finish_still_exits_zero() {
    with_a_request_in_flight(|service, _stream| {
        let signalled = Instant::now();
        service.signal("TERM");

        assert_eq!(service.wait_for_exit(), 0);

        // A drain that finished on its own means the starting state did not
        // build: there was no request in flight for the period to elapse over.
        if service.drain_outcome() != "timed-out" {
            return false;
        }

        let elapsed = signalled.elapsed();
        assert!(
            elapsed >= DRAIN_PERIOD,
            "the process left after {elapsed:?}, before the drain period elapsed"
        );
        true
    });
}

// BMS-FR-10: SIGINT shuts the service down on the same terms as SIGTERM.
#[cfg(unix)]
#[test]
fn sigint_shuts_down_with_status_zero() {
    let mut service = Service::start(local_bind);

    assert_eq!(request(service.address(), "GET", "/v1/health").status, 200);

    service.signal("INT");
    assert_eq!(service.wait_for_exit(), 0);
    assert_eq!(service.drain_outcome(), "drained");
}

// BMS-FR-10: a second signal during the drain leaves at once, without waiting
// for the drain period, and still with status 0.
#[cfg(unix)]
#[test]
fn a_second_signal_during_the_drain_leaves_at_once() {
    with_a_request_in_flight(|service, _stream| {
        let signalled = Instant::now();
        service.signal("TERM");

        // The first signal starts a drain this connection cannot finish, so
        // without the second signal the process would stay for the whole
        // drain period.
        let draining = service.next_record();
        assert_eq!(draining["event"], "draining");

        service.signal("TERM");
        assert_eq!(service.wait_for_exit(), 0);

        // A drain that had nothing to wait on finished on its own, and there
        // was no drain for the second signal to interrupt.
        if service.drain_outcome() != "interrupted" {
            return false;
        }

        let elapsed = signalled.elapsed();
        assert!(
            elapsed < DRAIN_PERIOD,
            "the process waited {elapsed:?}, which is the whole drain period"
        );
        true
    });
}

// BMS-FR-10: a collector that closed the pipe must not turn a clean shutdown
// into a crash. `println!` panics on a broken pipe, which would leave the exit
// status at 101 instead of 0.
#[cfg(unix)]
#[test]
fn a_closed_standard_output_does_not_change_the_exit_status() {
    let mut service = Service::start(local_bind);
    assert_eq!(request(service.address(), "GET", "/v1/health").status, 200);

    service.close_standard_output();
    service.signal("TERM");

    assert_eq!(service.wait_for_exit(), 0);
}

// SAS-FR-VKTP: a missing administrator identifier, and a missing token, are
// each a startup failure with the configuration status of BMS-FR-07.
#[test]
fn a_missing_identity_setting_exits_two() {
    for (name, value) in [
        ("SYNTHESIS_SERVER_ADMIN_ID", ""),
        ("SYNTHESIS_SERVER_TOKEN", ""),
    ] {
        let child = spawn(&arguments(&[]), &variables(&[(name, value)]));
        let output = child.wait_with_output().expect("the process leaves");

        assert_eq!(output.status.code(), Some(2), "{name}");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            diagnostic.lines().count(),
            1,
            "one diagnostic: {diagnostic}"
        );
        assert!(diagnostic.contains(name), "{diagnostic}");
        assert!(output.stdout.is_empty(), "no record was written");
    }
}

// SAS-FR-HGZL: an administrator identifier that is not a UUID, and a token
// that is too short, are each refused before the listener is bound.
#[test]
fn a_malformed_identity_setting_exits_two() {
    for (name, value) in [
        ("SYNTHESIS_SERVER_ADMIN_ID", "administrator"),
        ("SYNTHESIS_SERVER_TOKEN", "short"),
    ] {
        let child = spawn(&arguments(&[]), &variables(&[(name, value)]));
        let output = child.wait_with_output().expect("the process leaves");

        assert_eq!(output.status.code(), Some(2), "{name}");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(diagnostic.contains(name), "{diagnostic}");
        assert!(output.stdout.is_empty(), "no record was written");
    }
}

// SAS-FR-PMRB: the token reaches no diagnostic and no record on standard
// output, whether the service starts or is refused.
#[test]
fn no_record_and_no_diagnostic_holds_the_token() {
    let service = Service::start(local_bind);
    let answer = request(service.address(), "GET", "/v1/health");
    assert_eq!(answer.status, 200);
    assert!(
        !String::from_utf8_lossy(&answer.raw).contains(DEFAULT_TOKEN),
        "the health answer holds the token"
    );

    let child = spawn(
        &arguments(&[]),
        &variables(&[("SYNTHESIS_SERVER_TOKEN", "short")]),
    );
    let output = child.wait_with_output().expect("the process leaves");
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(!diagnostic.contains("short"), "{diagnostic}");
}

// The non-functional logging rule of `SAS-server-application-service.md`: the
// running process writes one record per served application request, naming the
// route and the status, and the record holds no token.
#[test]
fn the_running_process_records_the_route_and_the_status_of_a_request() {
    let mut service = Service::start(local_bind);

    let answered = request_with_token(service.address(), "GET", "/v1/projects", DEFAULT_TOKEN);
    assert_eq!(answered.status, 200);

    let record = service.next_record();
    assert_eq!(record["event"], "request");
    assert_eq!(record["method"], "GET");
    assert_eq!(record["route"], "/v1/projects");
    assert_eq!(record["status"], 200);
    assert!(
        !record.to_string().contains(DEFAULT_TOKEN),
        "the record holds the token: {record}"
    );
}

// SAS-FR-LFCA: an application route needs the token, and the health route needs
// none. The service the test started holds the token of `DEFAULT_TOKEN`.
#[test]
fn the_application_routes_need_the_token_and_health_needs_none() {
    let service = Service::start(local_bind);

    let refused = request(service.address(), "GET", "/v1/projects");
    assert_eq!(refused.status, 401);
    let body = String::from_utf8_lossy(&refused.body);
    assert!(body.contains("unauthenticated"), "{body}");

    let answered = request_with_token(service.address(), "GET", "/v1/projects", DEFAULT_TOKEN);
    assert_eq!(answered.status, 200);
    let body = String::from_utf8_lossy(&answered.body);
    assert!(body.contains("items"), "{body}");

    let health = request(service.address(), "GET", "/v1/health");
    assert_eq!(health.status, 200);
}
