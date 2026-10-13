//! EAC-FR-RVQE — the real-runtime probe.
//!
//! Every other sandbox test binds a recording double, which proves what the
//! executor asks for and nothing about what a VM then gives the turn. These
//! tests boot a real microsandbox VM through the production executor, from a
//! probe image that stands in for the vendor CLI (`sandbox_probe/Dockerfile`).
//! The probe checks its execution directory, writes what it found to
//! `.probe/result.json`, and answers through `agentic-cli-mock`, so no real
//! agent runs and no token is spent.
//!
//! Ignored by default, because CI has no hardware virtualisation. They need a
//! running Docker daemon (to build and export the image) and a machine that
//! runs Microsandbox. Run them with:
//!
//! ```text
//! cd src-tauri && TMPDIR=/tmp cargo test -- --ignored sandbox_real
//! ```

use std::sync::OnceLock;

use super::sandbox::micro::MicrosandboxRuntime;
use super::sandbox::{SandboxPool, SandboxScope};
use super::*;
use crate::isolation::{ResolvedIsolation, SandboxSettings};

/// The tag the probe image is built under. Test-only.
const PROBE_IMAGE: &str = "synthesis-sandbox-probe:test";
const DOCKERFILE: &str = include_str!("sandbox_probe/Dockerfile");
const PROBE_SCRIPT: &str = include_str!("sandbox_probe/probe.sh");
/// The tracked file every fixture repository holds, and the probe reads.
const HOST_FILE: &str = "probe-host-file";
/// What the probe image puts on `PATH` alone (`sandbox_probe/Dockerfile`).
const PROBE_TOOL: &str = "/opt/probe/bin/probe-tool";

struct ProbeImages;

impl VendorImageSource for ProbeImages {
    fn image_for(&self, _vendor: &str) -> Result<String, String> {
        Ok(PROBE_IMAGE.to_string())
    }
}

static PROBE_IMAGES: ProbeImages = ProbeImages;

/// Build the probe image once per test process, from a temporary context that
/// holds the probe files and the mock crate alone.
fn build_probe_image() {
    static BUILT: OnceLock<()> = OnceLock::new();
    BUILT.get_or_init(|| {
        let context = tempfile::tempdir().expect("build context");
        let root = context.path();
        std::fs::write(root.join("Dockerfile"), DOCKERFILE).unwrap();
        std::fs::write(root.join("probe.sh"), PROBE_SCRIPT).unwrap();
        let scenario = json!({
            "version": 1,
            "expected": { "required_args": [] },
            "response": {
                "stdout": { "encoding": "utf8", "value": valid_claude_stdout() },
                "stderr": "",
                "exit_code": 0
            }
        });
        std::fs::write(root.join("scenario.template.json"), scenario.to_string()).unwrap();
        let mock = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the repository root")
            .join("tools/agentic-cli-mock");
        let target = root.join("agentic-cli-mock");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::copy(mock.join("Cargo.toml"), target.join("Cargo.toml")).unwrap();
        std::fs::copy(mock.join("Cargo.lock"), target.join("Cargo.lock")).unwrap();
        copy_tree(&mock.join("src"), &target.join("src"));
        let built = std::process::Command::new("docker")
            .args(["build", "--quiet", "-t", PROBE_IMAGE])
            .arg(root)
            .status()
            .expect("docker is on PATH");
        assert!(built.success(), "the probe image did not build");
    });
}

/// A small VM, so several probes can boot side by side.
fn microsandbox() -> ResolvedIsolation {
    ResolvedIsolation::Microsandbox(SandboxSettings {
        cpus: 1,
        memory_gib: 1,
        disk_gib: 16,
        block_local_network: true,
        allowed: Vec::new(),
        unsupported_reason: None,
    })
}

/// A scope no real stream uses, so a probe VM never replaces one of the
/// application's.
fn probe_scope() -> SandboxScope {
    SandboxScope { queue_key: format!("probe-{}", descriptor::unique_container_name()) }
}

/// The executor of one test. Under Microsandbox it boots real VMs through
/// `pool`, and reads the image ID and the export from the local Docker daemon.
fn executor(
    harness: &Harness,
    isolation: ResolvedIsolation,
    pool: &Arc<SandboxPool>,
) -> AgentCliExecutor {
    AgentCliExecutor::new(Arc::new(HostDockerCli::with_program("docker")))
        .with_session_state_root(harness.sessions_root())
        .with_isolation(isolation)
        .with_sandbox_pool(pool.clone())
        .with_image_exporter(Arc::new(HostDockerCli::with_program("docker")))
}

fn real_pool() -> Arc<SandboxPool> {
    Arc::new(SandboxPool::new(Arc::new(MicrosandboxRuntime::default())))
}

/// Stops every VM of a pool when a test ends, also when an assertion failed,
/// so a failed probe leaves no VM running. It stops them on the runtime the
/// test booted them on.
struct StopOnDrop<'a>(&'a tokio::runtime::Runtime, Arc<SandboxPool>);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        let pool = self.1.clone();
        self.0.block_on(async move { pool.stop_all().await });
    }
}

/// One multi-thread runtime for a whole test: a VM handle that a launch
/// creates must outlive that launch.
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime")
}

/// One probe turn in `directory`. Returns what the probe found.
async fn probe(
    executor: &AgentCliExecutor,
    harness: &Harness,
    directory: &Path,
    turn_kind: TurnKind,
    scope: &SandboxScope,
) -> serde_json::Value {
    let context = LaunchContext { images: &PROBE_IMAGES, ..harness.context() };
    let result = std::path::PathBuf::from(directory).join(".probe/result.json");
    let _ = std::fs::remove_file(&result);
    let execution = executor
        .execute_agent_cli(
            &Silent,
            &context,
            AgentExecutionRequest {
                sandbox_scope: Some(scope.clone()),
                turn_kind,
                execution_directory: directory.to_path_buf(),
                task: task("probe"),
                cancellation: CancellationToken::new(),
                activity: None,
                durable_output: None,
                supplementary_mount: None,
            },
        )
        .await
        .expect("the probe launches");
    let text = std::fs::read_to_string(&result).unwrap_or_else(|error| {
        panic!(
            "the probe wrote no result ({error}); outcome {:?}, stdout {}, stderr {}",
            execution.process_outcome,
            String::from_utf8_lossy(&execution.stdout.bytes),
            String::from_utf8_lossy(&execution.stderr.bytes)
        )
    });
    assert_eq!(
        execution.process_outcome,
        ProcessOutcome::Completed,
        "the probe turn did not complete; stderr {}",
        String::from_utf8_lossy(&execution.stderr.bytes)
    );
    serde_json::from_str(&text).expect("the probe result is JSON")
}

fn field<'a>(result: &'a serde_json::Value, name: &str) -> &'a str {
    result[name].as_str().unwrap_or_else(|| panic!("the probe result has no {name}"))
}

/// A linked worktree at `worktree`, with `HOST_FILE` committed in it. Returns
/// the commit.
fn repository(root: &Path, worktree: &Path, name: &str, content: &str) -> git2::Oid {
    std::fs::create_dir_all(worktree).unwrap();
    linked_worktree_named(root, worktree, name);
    commit(worktree, content, "probe fixture")
}

/// Commit `content` as `HOST_FILE` in the checkout at `worktree`.
fn commit(worktree: &Path, content: &str, message: &str) -> git2::Oid {
    let repo = git2::Repository::open(worktree).expect("the worktree opens");
    std::fs::write(worktree.join(HOST_FILE), content).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(HOST_FILE)).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let who = git2::Signature::now("t", "t@example.com").unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
    repo.commit(Some("HEAD"), &who, &who, message, &tree, &parents).unwrap()
}

/// Whether the host file carries an ownership attribute of the sandbox
/// runtime (EAC-FR-KTWO).
#[cfg(target_os = "macos")]
fn has_ownership_attribute(path: &Path) -> bool {
    let listed = std::process::Command::new("xattr").arg("-l").arg(path).output().unwrap();
    String::from_utf8_lossy(&listed.stdout).contains("override_stat")
}

#[cfg(not(target_os = "macos"))]
fn has_ownership_attribute(_path: &Path) -> bool {
    false
}

// EAC-FR-RVQE, EAC-FR-KTWO, EAC-FR-BJLI, EAC-FR-PWLA: a work turn in the VM
// writes its working copy as the host user, sees the image environment, and
// reads its repository read-only.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_work_turn_writes_as_the_host_user_and_reads_git() {
    use std::os::unix::fs::MetadataExt;

    build_probe_image();
    let harness = harness_for("claude_code");
    let root = tempfile::tempdir().unwrap();
    let head = repository(root.path(), &harness.workspace(), "work", "one\n");
    let host_file = harness.workspace().join(HOST_FILE);
    std::fs::set_permissions(&host_file, std::os::unix::fs::PermissionsExt::from_mode(0o640))
        .unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);
    let (uid, gid) = descriptor::host_ids();

    let result = rt.block_on(async {
        probe(&executor, &harness, &harness.workspace(), TurnKind::Work, &probe_scope()).await
    });

    assert_eq!(field(&result, "writable"), "yes", "{result}");
    assert_eq!(field(&result, "dir_writable"), "yes", "{result}");
    assert_eq!(field(&result, "uid"), uid.to_string(), "{result}");
    assert_eq!(field(&result, "gid"), gid.to_string(), "{result}");
    assert_eq!(field(&result, "host_owner"), uid.to_string(), "{result}");
    // The real group of the host file. A new file takes the group of its
    // directory on macOS, so it is not always the process group.
    let host_gid = std::fs::metadata(&host_file).unwrap().gid();
    assert_eq!(field(&result, "host_group"), host_gid.to_string(), "{result}");
    assert_eq!(field(&result, "host_mode"), "640", "{result}");
    // EAC-FR-WBDE: the generated store takes a write, the real store does not
    // see it, and the `.git` pin takes none.
    assert_eq!(field(&result, "store_writable"), "yes", "{result}");
    assert!(!root.path().join("primary/.git/probe-store-write-earlier").exists());
    assert_eq!(field(&result, "pin_writable"), "no", "{result}");
    assert_eq!(field(&result, "tool"), PROBE_TOOL, "{result}");
    assert_eq!(field(&result, "home"), "/home/agent", "{result}");
    assert_eq!(field(&result, "optional_locks"), "0", "{result}");
    assert_eq!(field(&result, "head"), head.to_string(), "{result}");
    assert_eq!(field(&result, "git_dir_writable"), "no", "{result}");
    assert_eq!(field(&result, "config_bytes"), "0", "{result}");
    // EAC-FR-QDVN: the worktree mount and the root disk each offer the disk
    // size of the isolation (16 GiB here), not the runtime's 4 GiB and not the
    // whole host disk.
    let kib = |name: &str| field(&result, name).parse::<u64>().unwrap_or(0);
    let gib = 1024 * 1024;
    for name in ["workspace_kib", "root_kib"] {
        assert!((15 * gib..=17 * gib).contains(&kib(name)), "{name}: {result}");
    }

    let written = harness.workspace().join(".probe/written");
    let meta = std::fs::metadata(&written).unwrap();
    assert_eq!(meta.uid(), uid);
    assert!(
        meta.gid() == gid || meta.gid() == host_gid,
        "the host user's group or the directory's"
    );
    // The mode the guest gave the file is the mode the host holds.
    assert_eq!(format!("{:o}", meta.mode() & 0o777), field(&result, "written_mode"));
    assert!(!has_ownership_attribute(&written), "the runtime wrote an ownership attribute");
}

// EAC-FR-RVQE, EAC-FR-BJLI: a review turn sees the change that stands
// uncommitted against HEAD in its checkout.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_review_turn_sees_the_uncommitted_change() {
    build_probe_image();
    let harness = harness_for("claude_code");
    let root = tempfile::tempdir().unwrap();
    repository(root.path(), &harness.workspace(), "rv", "one\n");
    std::fs::write(harness.workspace().join(HOST_FILE), "changed\n").unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);

    let result = rt.block_on(async {
        probe(&executor, &harness, &harness.workspace(), TurnKind::Review, &probe_scope()).await
    });

    assert_eq!(field(&result, "diff"), HOST_FILE, "{result}");
}

// EAC-FR-RVQE, EAC-FR-UEFI: two sequential turns of one scope share the VM, and
// the second sees the commit the host made between them.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_sequential_turns_see_the_new_head() {
    build_probe_image();
    let harness = harness_for("claude_code");
    let root = tempfile::tempdir().unwrap();
    let first = repository(root.path(), &harness.workspace(), "seq", "one\n");
    // Present from the start, so it is mounted, and replaced by a rename below.
    let packed = root.path().join("primary/.git/packed-refs");
    std::fs::write(&packed, "# pack-refs with: peeled fully-peeled sorted\n").unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);
    let scope = probe_scope();
    let workspace = harness.workspace();

    let (before, after, second) = rt.block_on(async {
        let before = probe(&executor, &harness, &workspace, TurnKind::Work, &scope).await;
        let second = commit(&workspace, "two\n", "the host commits the run's work");
        let replacement = packed.with_extension("new");
        std::fs::write(
            &replacement,
            format!(
                "# pack-refs with: peeled fully-peeled sorted\n{second} refs/heads/probe-packed\n"
            ),
        )
        .unwrap();
        std::fs::rename(&replacement, &packed).unwrap();
        let after = probe(&executor, &harness, &workspace, TurnKind::Work, &scope).await;
        (before, after, second)
    });

    assert_eq!(field(&before, "head"), first.to_string(), "{before}");
    assert_eq!(field(&after, "head"), second.to_string(), "{after}");
    // EAC-FR-UEFI, EAC-FR-JRVY: the second turn ran in the same VM, and a file
    // the host replaced by a rename shows its new content there.
    assert_eq!(field(&before, "boot_id"), field(&after, "boot_id"), "the VM was reused");
    assert_eq!(field(&after, "packed"), second.to_string(), "{after}");
    // EAC-FR-WBDE: what the first turn wrote to the generated store is gone.
    assert_eq!(field(&before, "store_writable"), "yes", "{before}");
    assert_eq!(field(&after, "store_leftover"), "no", "{after}");
}

// EAC-FR-RVQE, EAC-FR-HXDN: a review checkout deleted and created again at the
// same path shows its new contents to the next review turn.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_recreated_review_checkout_shows_its_new_contents() {
    build_probe_image();
    let harness = harness_for("claude_code");
    let workspace = harness.workspace();
    let checkout = workspace.join("rv");
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);
    let scope = probe_scope();

    let (before, after, second) = rt.block_on(async {
        repository(first_root.path(), &checkout, "rv-1", "one\n");
        let before = probe(&executor, &harness, &checkout, TurnKind::Review, &scope).await;
        std::fs::remove_dir_all(&checkout).unwrap();
        let second = repository(second_root.path(), &checkout, "rv-2", "other\n");
        let after = probe(&executor, &harness, &checkout, TurnKind::Review, &scope).await;
        (before, after, second)
    });

    assert_ne!(field(&before, "head"), second.to_string(), "{before}");
    assert_eq!(field(&after, "head"), second.to_string(), "{after}");
}

// EAC-FR-RVQE, EAC-FR-BJLI: a merge_work turn reads the other side of the
// merge with `git show`.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_merge_work_turn_reads_the_other_side() {
    build_probe_image();
    let harness = harness_for("claude_code");
    let root = tempfile::tempdir().unwrap();
    let workspace = harness.workspace();
    let other = repository(root.path(), &workspace, "mw", "theirs\n");
    commit(&workspace, "ours\n", "the base side");
    std::fs::write(workspace.join("probe-show"), format!("{other}:{HOST_FILE}")).unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);

    let result = rt.block_on(async {
        probe(&executor, &harness, &workspace, TurnKind::MergeWork, &probe_scope()).await
    });

    assert_eq!(field(&result, "show"), "theirs", "{result}");
}

// EAC-FR-RVQE: a work turn and a review turn find the same things in the VM as
// they find in a Docker container.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_turns_match_docker_turns() {
    const COMPARED: [&str; 12] = [
        "uid",
        "writable",
        "dir_writable",
        "tool",
        "home",
        "host_mode",
        "pin_writable",
        "optional_locks",
        "head",
        "diff",
        "git_dir_writable",
        "config_bytes",
    ];
    build_probe_image();
    let harness = harness_for("claude_code");
    let root = tempfile::tempdir().unwrap();
    let workspace = harness.workspace();
    repository(root.path(), &workspace, "parity", "one\n");
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let sandboxed = executor(&harness, microsandbox(), &pool);
    let docker = executor(&harness, ResolvedIsolation::None, &pool);
    let scope = probe_scope();

    rt.block_on(async {
        for turn_kind in [TurnKind::Work, TurnKind::Review] {
            std::fs::write(workspace.join(HOST_FILE), format!("{turn_kind:?}\n")).unwrap();
            let in_vm = probe(&sandboxed, &harness, &workspace, turn_kind, &scope).await;
            let in_container = probe(&docker, &harness, &workspace, turn_kind, &scope).await;
            for name in COMPARED {
                assert_eq!(
                    field(&in_vm, name),
                    field(&in_container, name),
                    "{turn_kind:?}: {name} differs; VM {in_vm}, container {in_container}"
                );
            }
        }
    });
}

// EAC-FR-RVQE, EAC-FR-HXDN: a work turn's directory made again at the same
// path between two launches of one scope shows its new contents.
#[test]
#[ignore = "needs hardware virtualisation and a Docker daemon"]
fn sandbox_real_recreated_work_directory_shows_its_new_contents() {
    build_probe_image();
    let harness = harness_for("claude_code");
    let checkout = harness.workspace().join("work");
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let pool = real_pool();
    let rt = runtime();
    let _stop = StopOnDrop(&rt, pool.clone());
    let executor = executor(&harness, microsandbox(), &pool);
    let scope = probe_scope();

    let (before, after, second) = rt.block_on(async {
        repository(first_root.path(), &checkout, "work-1", "one\n");
        let before = probe(&executor, &harness, &checkout, TurnKind::Work, &scope).await;
        // Renamed aside rather than deleted, so the inode cannot be given back.
        std::fs::rename(&checkout, harness.workspace().join("work-aside")).unwrap();
        let second = repository(second_root.path(), &checkout, "work-2", "other\n");
        let after = probe(&executor, &harness, &checkout, TurnKind::Work, &scope).await;
        (before, after, second)
    });

    assert_eq!(field(&after, "head"), second.to_string(), "{after}");
    assert_ne!(field(&before, "boot_id"), field(&after, "boot_id"), "the VM was rebooted");
}
