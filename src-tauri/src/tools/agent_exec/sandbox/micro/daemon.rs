//! EAC-FR-QMRW / EAC-FR-ZPTA: the start of the image's `dockerd` in a booted
//! VM. Split out of `micro.rs` for size alone.

use std::time::Duration;

use microsandbox::ExecEvent;

use super::MicrosandboxRuntime;

/// EAC-FR-QMRW: how long the daemon may take to answer on its socket.
const DAEMON_READY_SECONDS: u32 = 60;
/// The bound on the whole script: the readiness bound and the time to start
/// the shell and print the log tail.
const DAEMON_EXEC_BOUND: Duration = Duration::from_secs(DAEMON_READY_SECONDS as u64 + 15);
/// The most output of the script that is kept for the failure message.
const OUTPUT_LIMIT: usize = 4096;

/// EAC-FR-QMRW: the guest socket of the daemon.
const GUEST_SOCKET: &str = "/var/run/docker.sock";
/// EAC-FR-ZPTA: the guest log of the daemon, whose end a failure reports.
const GUEST_LOG: &str = "/var/log/synthesis-dockerd.log";

/// EAC-FR-QMRW: start `dockerd` (`$5`) as root, detached in a session of its
/// own, with its socket at `$3` and `$1` as the group of the socket, then wait
/// up to `$2` seconds until it answers. A daemon that already answers, as in a
/// reused VM, is kept and the script ends at once. The exit code tells the
/// cause (EAC-FR-ZPTA): 127 no `dockerd`, 3 `dockerd` exited, 4 not ready in
/// time. The log of the daemon (`$4`) stays in the guest, and its end is
/// printed on a failure.
const START_DOCKERD: &str = r#"
group="$1"; ready_seconds="$2"; sock="$3"; log="$4"; dockerd="$5"
ready() {
  [ -S "$sock" ] || return 1
  if command -v curl >/dev/null 2>&1; then
    curl -fsS --max-time 2 --unix-socket "$sock" http://localhost/_ping >/dev/null 2>&1
    return $?
  fi
  if command -v docker >/dev/null 2>&1; then
    docker -H "unix://$sock" version >/dev/null 2>&1
    return $?
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$sock" >/dev/null 2>&1 <<'PY'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.settimeout(2)
s.connect(sys.argv[1])
s.sendall(b"GET /_ping HTTP/1.0\r\nHost: docker\r\n\r\n")
reply = s.recv(64)
sys.exit(0 if reply.startswith(b"HTTP/1.") and b" 200 " in reply else 1)
PY
    return $?
  fi
  return 0
}
if ready; then
  exit 0
fi
if ! command -v "$dockerd" >/dev/null 2>&1; then
  echo "dockerd is not in the image"
  exit 127
fi
mkdir -p "$(dirname "$log")" "$(dirname "$sock")"
if [ "$(id -u)" = 0 ] && ! grep -q ' /sys/fs/cgroup ' /proc/mounts; then
  mkdir -p /sys/fs/cgroup
  mount -t cgroup2 none /sys/fs/cgroup 2>/dev/null || true
fi
rm -f "$sock"
setsid "$dockerd" --group "$group" --host "unix://$sock" >"$log" 2>&1 </dev/null &
pid=$!
running() {
  state=$(cut -d' ' -f3 "/proc/$pid/stat" 2>/dev/null)
  [ -n "$state" ] && [ "$state" != "Z" ]
}
ticks=$((ready_seconds * 10))
i=0
while [ "$i" -lt "$ticks" ]; do
  if ! running; then
    echo "dockerd exited: $(tail -c 600 "$log" | tr '\n' ' ')"
    exit 3
  fi
  if ready; then
    exit 0
  fi
  sleep 0.1
  i=$((i + 1))
done
echo "dockerd was not ready after ${ready_seconds} s: $(tail -c 600 "$log" | tr '\n' ' ')"
exit 4
"#;

impl MicrosandboxRuntime {
    /// EAC-FR-QMRW: run [`START_DOCKERD`] in the running `vm` as root. An error
    /// is the script's own words, or why the script did not run.
    pub(super) async fn docker_daemon_exec(&self, vm: &str, group: u32) -> Result<(), String> {
        let vms = self.vms.lock().await;
        let Some(booted) = vms.get(vm) else {
            return Err("the sandbox VM is not running".to_string());
        };
        let args = vec![
            "-c".to_string(),
            START_DOCKERD.to_string(),
            "synthesis-dockerd".to_string(),
            group.to_string(),
            DAEMON_READY_SECONDS.to_string(),
            GUEST_SOCKET.to_string(),
            GUEST_LOG.to_string(),
            "dockerd".to_string(),
        ];
        let mut handle = booted
            .sandbox
            .exec_stream_with("/bin/sh", move |o| o.args(args).user("0:0").tty(false).stdin_null())
            .await
            .map_err(|e| format!("the daemon start did not run: {e}"))?;
        drop(vms);
        let mut output = Vec::new();
        let ended = tokio::time::timeout(DAEMON_EXEC_BOUND, async {
            while let Some(event) = handle.recv().await {
                match event {
                    ExecEvent::Stdout(bytes) | ExecEvent::Stderr(bytes) => {
                        let room = OUTPUT_LIMIT.saturating_sub(output.len());
                        output.extend_from_slice(&bytes[..bytes.len().min(room)]);
                    }
                    ExecEvent::Exited { code } => return Some(code),
                    ExecEvent::Failed(_) => return None,
                    _ => {}
                }
            }
            None
        })
        .await;
        // The log tail of the daemon reaches the application log and the run
        // detail, so a credential in a URL of the image's own configuration is
        // removed first.
        let said = redact_url_credentials(String::from_utf8_lossy(&output).trim());
        match ended {
            Ok(Some(0)) => Ok(()),
            Ok(Some(code)) if !said.is_empty() => Err(format!("{said} (exit {code})")),
            Ok(Some(code)) => Err(format!("the daemon start exited with {code}")),
            Ok(None) => Err("the daemon start did not run".to_string()),
            Err(_) => {
                let _ = handle.kill().await;
                Err(format!("dockerd was not ready after {DAEMON_READY_SECONDS} s: {said}"))
            }
        }
    }
}

/// Remove the user information of every URL (`scheme://user:pass@host` gives
/// `scheme://***@host`), so a proxy or a registry mirror credential that the
/// daemon prints never reaches a log.
fn redact_url_credentials(text: &str) -> String {
    static USERINFO: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern =
        USERINFO.get_or_init(|| regex::Regex::new(r"://[^/\s@]+@").expect("a valid pattern"));
    pattern.replace_all(text, "://***@").into_owned()
}

#[cfg(test)]
mod tests {
    use super::redact_url_credentials;
    #[cfg(target_os = "linux")]
    use super::START_DOCKERD;

    // EAC-FR-ZPTA: the daemon's own words reach the message without a URL
    // credential.
    #[test]
    fn url_credentials_are_removed_from_the_daemon_words() {
        assert_eq!(
            redact_url_credentials(
                "dockerd exited: proxy http://bob:s3cret@proxy.local:3128 and https://t@reg.io/v2 failed"
            ),
            "dockerd exited: proxy http://***@proxy.local:3128 and https://***@reg.io/v2 failed"
        );
        assert_eq!(
            redact_url_credentials("dockerd is not in the image"),
            "dockerd is not in the image"
        );
        assert_eq!(
            redact_url_credentials("see https://docs.docker.com/x"),
            "see https://docs.docker.com/x"
        );
    }

    /// Run the start script on this host with `dockerd` as the daemon program,
    /// a bound of one second, and a socket and a log in `dir`.
    #[cfg(target_os = "linux")]
    fn start(dir: &std::path::Path, dockerd: &str) -> (Option<i32>, String) {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(START_DOCKERD)
            .arg("synthesis-dockerd")
            .arg("1000")
            .arg("1")
            .arg(dir.join("run/docker.sock"))
            .arg(dir.join("log/dockerd.log"))
            .arg(dockerd)
            .output()
            .expect("sh runs");
        (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[cfg(target_os = "linux")]
    fn stub(dir: &std::path::Path, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("dockerd-stub");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    // EAC-FR-ZPTA: an image with no `dockerd` ends with 127 and says so.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_start_script_reports_a_missing_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let (code, said) = start(dir.path(), "/nonexistent/dockerd");
        assert_eq!(code, Some(127));
        assert_eq!(said.trim(), "dockerd is not in the image");
    }

    // EAC-FR-ZPTA: a daemon that exits ends with 3 and gives the end of its log.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_start_script_reports_a_daemon_that_exits_with_its_log() {
        let dir = tempfile::tempdir().unwrap();
        let program = stub(dir.path(), "echo stub-daemon-failed; exit 1");
        let (code, said) = start(dir.path(), &program);
        assert_eq!(code, Some(3), "{said}");
        assert!(said.starts_with("dockerd exited:"), "{said}");
        assert!(said.contains("stub-daemon-failed"), "{said}");
    }

    // EAC-FR-ZPTA: a daemon that never answers ends with 4 after the bound and
    // gives the end of its log.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_start_script_reports_a_daemon_that_does_not_answer() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("stub.pid");
        let program = stub(
            dir.path(),
            &format!("echo stub-daemon-waits; echo $$ > {}; exec sleep 60", pid.display()),
        );
        let (code, said) = start(dir.path(), &program);
        if let Ok(text) = std::fs::read_to_string(&pid) {
            let _ = std::process::Command::new("kill").arg(text.trim()).status();
        }
        assert_eq!(code, Some(4), "{said}");
        assert!(said.starts_with("dockerd was not ready after 1 s:"), "{said}");
        assert!(said.contains("stub-daemon-waits"), "{said}");
    }
}
