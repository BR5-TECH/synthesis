//! The executable. Everything of substance lives in the library so it can be
//! unit-tested without spawning a process; this is only the wiring to the real
//! streams, the real clock, and the real exit status.

use std::ffi::OsString;
use std::io;
use std::process;
use std::thread;
use std::time::Duration;

use agentic_cli_mock::{run, Environment};

fn main() {
    // `args_os`, not `args`: an argument that is not valid Unicode must reach
    // the comparison as the bytes the operating system delivered rather than
    // panicking or being replaced (ACM-FR-04).
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();

    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    let sleep = |duration: Duration| thread::sleep(duration);

    let mut environment = Environment {
        stdin: &mut stdin,
        stdout: &mut stdout,
        stderr: &mut stderr,
        sleep: &sleep,
    };

    let code = run(&argv, agentic_cli_mock::adapter::REGISTRY, &mut environment);

    // `run` has already flushed both streams. `process::exit` runs no
    // destructors, which is why that flush is explicit rather than implied.
    process::exit(i32::from(code));
}
