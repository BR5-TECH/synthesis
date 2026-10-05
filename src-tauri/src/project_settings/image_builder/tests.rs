//! The tests of the image builder: what a build context holds, what a sink
//! receives, and how a build ends.

use super::*;
use std::sync::Mutex;

#[derive(Default)]
struct RecordingSink {
    updates: Mutex<Vec<(String, Option<u64>, Option<u64>, String)>>,
}

impl ImageBuildSink for RecordingSink {
    fn progress(&self, phase: &str, completed: Option<u64>, total: Option<u64>, message: &str) {
        self.updates.lock().unwrap().push((
            phase.to_string(),
            completed,
            total,
            message.to_string(),
        ));
    }
    fn cancelled(&self) -> bool {
        false
    }
}

/// PSS-FR-28 (PSS-FR-27): indeterminate until a total arrives, determinate
/// afterwards, and never back.
#[test]
fn progress_becomes_determinate_and_stays_there() {
    let sink = RecordingSink::default();
    let mut position = None;
    let mut tail = Vec::new();
    report(&sink, "Sending build context to Docker daemon\n", &mut position, &mut tail);
    report(&sink, "Step 1/3 : FROM debian\n", &mut position, &mut tail);
    report(&sink, " ---> Running in c0ffee\n", &mut position, &mut tail);
    report(&sink, "Step 2/3 : RUN true\n", &mut position, &mut tail);

    let updates = sink.updates.lock().unwrap().clone();
    assert_eq!(updates[0].1, None, "the first update carries no position");
    assert_eq!(updates[0].2, None);
    assert_eq!((updates[1].1, updates[1].2), (Some(1), Some(3)));
    assert_eq!(
        (updates[2].1, updates[2].2),
        (Some(1), Some(3)),
        "a line with no position of its own keeps the last one"
    );
    assert_eq!((updates[3].1, updates[3].2), (Some(2), Some(3)));
    assert!(updates.iter().all(|u| u.0 == PHASE_BUILDING));
}

/// PSS-FR-29 (PSS-FR-28): the diagnostic is the build's own output,
/// bounded.
#[test]
fn a_failure_diagnostic_is_the_builds_own_tail() {
    let tail: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
    let text = diagnostic_from(&tail, Some(1));
    assert!(text.lines().count() <= DIAGNOSTIC_LINES);
    assert!(text.contains("line 39"));
    assert!(!text.contains("line 0"));

    // A build that said nothing still says something.
    assert_eq!(
        diagnostic_from(&[], Some(125)),
        "the build failed with exit status 125"
    );
}

/// PSS-FR-26 (PSS-FR-26): a CLI build runs the verified executable and
/// nothing else, and a failure is reported as the build's own.
#[cfg(unix)]
#[test]
fn a_cli_build_reports_what_the_executable_did() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("fake-docker");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho \"Step 1/2 : FROM scratch\"\necho \"Step 2/2 : RUN true\"\nexit 0\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let request = ImageBuildRequest {
        vendor: "claude_code".into(),
        operation_id: "op-1".into(),
        image_reference: "acme/agent:1".into(),
        dockerfile: "Dockerfile".into(),
        context_root: dir.path().to_path_buf(),
    };
    let sink = RecordingSink::default();
    assert_eq!(
        build_with_cli(fake.to_str().unwrap(), &request, &sink),
        BuildTerminal::Succeeded
    );
    let updates = sink.updates.lock().unwrap().clone();
    assert!(updates.iter().any(|u| u.3.contains("Step 1/2")));
    assert_eq!(updates.last().unwrap().2, Some(2));

    let failing = dir.path().join("failing-docker");
    std::fs::write(&failing, "#!/bin/sh\necho 'no such file' 1>&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&failing, std::fs::Permissions::from_mode(0o755)).unwrap();
    let sink = RecordingSink::default();
    match build_with_cli(failing.to_str().unwrap(), &request, &sink) {
        BuildTerminal::Failed(text) => assert!(text.contains("no such file"), "{text}"),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// PSS-FR-29 (PSS-FR-28): a cancellation stops the build and is reported as
/// one.
#[cfg(unix)]
#[test]
fn a_cancelled_cli_build_reports_cancelled() {
    struct AlwaysCancelled;
    impl ImageBuildSink for AlwaysCancelled {
        fn progress(&self, _: &str, _: Option<u64>, _: Option<u64>, _: &str) {}
        fn cancelled(&self) -> bool {
            true
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let slow = dir.path().join("slow-docker");
    std::fs::write(&slow, "#!/bin/sh\nsleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();

    let request = ImageBuildRequest {
        vendor: "codex".into(),
        operation_id: "op-2".into(),
        image_reference: "acme/agent".into(),
        dockerfile: "Dockerfile".into(),
        context_root: dir.path().to_path_buf(),
    };
    assert_eq!(
        build_with_cli(slow.to_str().unwrap(), &request, &AlwaysCancelled),
        BuildTerminal::Cancelled
    );
}

// -- the build context (PSS-FR-26) -----------------------------------

/// The archive `pack_context` produces for `root`, as the entries it holds
/// with the mode each carries.
fn packed(root: &Path) -> Vec<(String, u32)> {
    let mut buffer: Vec<u8> = Vec::new();
    pack_context(root, &mut buffer).expect("the context packs");
    let mut archive = tar::Archive::new(std::io::Cursor::new(buffer));
    archive
        .entries()
        .expect("entries")
        .map(|entry| {
            let entry = entry.expect("an entry");
            let path = entry.path().expect("a path").to_string_lossy().into_owned();
            (path, entry.header().mode().expect("a mode"))
        })
        .collect()
}

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// PSS-FR-26 (PSS-FR-26): the repository's own object store is never part
/// of a build context.
#[test]
fn the_git_directory_is_never_packed() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Dockerfile", "FROM scratch\n");
    write(dir.path(), ".git/config", "[core]\n");
    write(dir.path(), ".git/objects/ab/cdef", "binary");
    write(dir.path(), "src/main.rs", "fn main() {}\n");

    let entries = packed(dir.path());
    let names: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(names.contains(&"Dockerfile"), "{names:?}");
    assert!(names.contains(&"src/main.rs"), "{names:?}");
    assert!(
        !names.iter().any(|p| p.starts_with(".git")),
        "the object store reached the context: {names:?}"
    );
}

/// PSS-FR-26 (PSS-FR-26): a project's own `.dockerignore` decides what else
/// is left out, so a build on an ordinary project sends what Docker would
/// send.
#[test]
fn dockerignore_entries_are_excluded() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".dockerignore", "# comment\nnode_modules\n*.log\ntarget/\n!keep.log\n");
    write(dir.path(), "Dockerfile", "FROM scratch\n");
    write(dir.path(), "node_modules/left-pad/index.js", "module.exports = 1;\n");
    write(dir.path(), "target/debug/huge", "0".repeat(10).as_str());
    write(dir.path(), "build.log", "noisy\n");
    write(dir.path(), "keep.log", "wanted\n");
    write(dir.path(), "src/app.ts", "export {};\n");

    let entries = packed(dir.path());
    let names: Vec<&str> = entries.iter().map(|(p, _)| p.as_str()).collect();
    assert!(names.contains(&"Dockerfile"), "{names:?}");
    assert!(names.contains(&"src/app.ts"), "{names:?}");
    // The ignore file itself is sent, exactly as Docker sends it.
    assert!(names.contains(&".dockerignore"), "{names:?}");
    assert!(
        !names.iter().any(|p| p.starts_with("node_modules")),
        "{names:?}"
    );
    assert!(!names.iter().any(|p| p.starts_with("target")), "{names:?}");
    assert!(!names.contains(&"build.log"), "{names:?}");
    // A `!` rule re-includes what the rule above it excluded.
    assert!(names.contains(&"keep.log"), "{names:?}");
}

/// PSS-FR-26 (PSS-FR-26): a file that is executable in the working copy is
/// executable in the build context, so a Dockerfile that copies a script
/// and runs it works under this backend exactly as it does under the Docker
/// CLI.
#[cfg(unix)]
#[test]
fn an_executable_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Dockerfile", "FROM scratch\n");
    write(dir.path(), "scripts/entry.sh", "#!/bin/sh\n");
    write(dir.path(), "README.md", "# hello\n");
    std::fs::set_permissions(
        dir.path().join("scripts/entry.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let entries = packed(dir.path());
    let mode = |name: &str| {
        entries
            .iter()
            .find(|(p, _)| p == name)
            .unwrap_or_else(|| panic!("{name} is in the context: {entries:?}"))
            .1
    };
    assert_eq!(mode("scripts/entry.sh") & 0o111, 0o111, "the script is executable");
    assert_eq!(mode("README.md") & 0o111, 0, "an ordinary file is not");
}

/// The `.dockerignore` subset, on its own terms.
#[test]
fn the_ignore_patterns_match_the_way_docker_spells_them() {
    let ignore = DockerIgnore::parse(
        "\n# a comment\nnode_modules\n*.log\ndocs/**/draft.md\nbuild/\n!build/keep\n",
    );
    assert!(ignore.excludes(Path::new("node_modules")));
    assert!(ignore.excludes(Path::new("node_modules/left-pad/index.js")));
    assert!(ignore.excludes(Path::new("app.log")));
    assert!(!ignore.excludes(Path::new("app.log.txt")));
    assert!(ignore.excludes(Path::new("docs/a/b/draft.md")));
    assert!(ignore.excludes(Path::new("docs/draft.md")));
    assert!(!ignore.excludes(Path::new("docs/final.md")));
    assert!(ignore.excludes(Path::new("build/artifact")));
    // The last matching rule decides, which is what makes `!` a
    // re-include rather than a second exclusion.
    assert!(!ignore.excludes(Path::new("build/keep")));
    assert!(ignore.may_reinclude_below(Path::new("build")));
    assert!(!ignore.may_reinclude_below(Path::new("node_modules/left-pad")));
    // An empty file excludes nothing at all.
    assert!(!DockerIgnore::parse("").excludes(Path::new("anything")));

    // A leading `**` takes no segment as readily as it takes several, which
    // is the form that names one directory wherever a tree repeats it.
    let anywhere = DockerIgnore::parse("**/target\n");
    assert!(anywhere.excludes(Path::new("target")));
    assert!(anywhere.excludes(Path::new("target/debug/binary")));
    assert!(anywhere.excludes(Path::new("crates/one/target/debug/binary")));
    assert!(!anywhere.excludes(Path::new("src/targets.rs")));
}

/// The repository's own `.dockerignore`, read back into what it means.
///
/// The file is what keeps the build context to the repository's authored
/// work. Without it the context is this machine's compiler and package
/// output — tens of gigabytes of `target/` — which the Docker Engine
/// cannot read before its own request bound elapses, and the author is
/// shown a timeout in place of a build. The meaning is asserted here rather
/// than trusted, because deleting the file breaks a build in a way that
/// names neither the file nor the size.
///
/// This measures the file against the documented subset of Docker's matcher
/// in this module, not against Docker. The two agree on every form the file
/// uses — a bare name, `*.ext`, and a leading `**/` — and the Docker CLI
/// backend reads the same file with Docker's own matcher.
#[test]
fn the_repository_dockerignore_keeps_build_output_out_of_the_context() {
    let ignore = DockerIgnore::parse(&repository_dockerignore());

    for excluded in [
        "src-tauri/target",
        "src-tauri/target/debug/synthesis",
        "server/target/release/synthesis-server",
        "tools/agentic-cli-mock/target/debug/agentic-cli-mock",
        "node_modules/react/index.js",
        "dist/index.html",
        "tsconfig.test.tsbuildinfo",
    ] {
        assert!(
            ignore.excludes(Path::new(excluded)),
            "{excluded} belongs to no build context"
        );
    }

    // A Dockerfile named inside the context is what the Engine backend
    // builds, so no pattern may exclude one.
    for kept in [
        "docker/agent-images/claude-code/Dockerfile",
        "docker/agent-images/codex/Dockerfile",
        "server/Dockerfile",
        "src/App.tsx",
        "src-tauri/src/lib.rs",
        "package.json",
    ] {
        assert!(
            !ignore.excludes(Path::new(kept)),
            "{kept} is part of the project and stays in the context"
        );
    }

    // Excluding the directory is only half of what makes the build quick.
    // The other half is that the packer never descends into it, which it
    // does only while no `!` rule could re-include something below. One
    // negated rule that holds `**` makes that true of **every** directory
    // at every depth, and the walk reads all of `target` again — for an
    // archive that is the same size, in the minutes this file removed.
    for pruned in ["src-tauri/target", "server/target", "node_modules"] {
        assert!(
            !ignore.may_reinclude_below(Path::new(pruned)),
            "{pruned} is walked rather than skipped"
        );
    }

    // `dist` is anchored at the root and `target` is not, which is
    // deliberate rather than an oversight: Docker anchors a pattern at the
    // context root, the frontend's output is the one at the root, and a
    // `dist` directory inside a source tree is that tree's own work.
    assert!(ignore.excludes(Path::new("dist/index.html")));
    assert!(!ignore.excludes(Path::new("src/dist/bundle.js")));
}

/// PSS-FR-26 (PSS-FR-26): the repository's own `.dockerignore`, through the
/// packer that reads it.
///
/// The test above asks what the file means. This one asks what the packer
/// sends, on a tree shaped like this repository, so a change that keeps the
/// patterns and loses the pruning is caught here rather than by a build
/// that takes minutes to say so.
#[test]
fn the_repository_dockerignore_packs_only_authored_work() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), DOCKERIGNORE, &repository_dockerignore());
    for kept in [
        "docker/agent-images/claude-code/Dockerfile",
        "src/App.tsx",
        "src-tauri/src/lib.rs",
        "package.json",
    ] {
        write(dir.path(), kept, "authored\n");
    }
    for excluded in [
        "src-tauri/target/debug/synthesis",
        "server/target/release/synthesis-server",
        "tools/agentic-cli-mock/target/debug/agentic-cli-mock",
        "node_modules/react/index.js",
        "dist/index.html",
        "tsconfig.test.tsbuildinfo",
    ] {
        write(dir.path(), excluded, "build output\n");
    }

    let packed: Vec<String> = packed(dir.path()).into_iter().map(|(p, _)| p).collect();
    for kept in [
        "docker/agent-images/claude-code/Dockerfile",
        "src/App.tsx",
        "src-tauri/src/lib.rs",
        "package.json",
    ] {
        assert!(packed.iter().any(|p| p == kept), "{kept} is missing: {packed:?}");
    }
    for gone in ["target", "node_modules", "dist/", "tsbuildinfo"] {
        assert!(
            !packed.iter().any(|p| p.contains(gone)),
            "{gone} was packed: {packed:?}"
        );
    }
}

/// PSS-FR-26 (PSS-FR-26): a project with no `.dockerignore` sends its whole
/// tree, and that is a decision rather than an omission.
///
/// The packer invents no exclusion a project did not ask for, because a
/// build context is not the place to guess what a Dockerfile needs. So a
/// repository that loses its `.dockerignore` is a repository that sends its
/// build output — which is what the file at this repository's root exists
/// to prevent, and why the two tests above refuse to run without it.
#[test]
fn a_project_with_no_dockerignore_sends_everything_it_holds() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Dockerfile", "FROM scratch\n");
    write(dir.path(), "target/debug/enormous", "0");

    let packed: Vec<String> = packed(dir.path()).into_iter().map(|(p, _)| p).collect();
    assert!(
        packed.iter().any(|p| p == "target/debug/enormous"),
        "nothing is excluded that the project did not exclude: {packed:?}"
    );
}

/// The text of the repository's own `.dockerignore`.
///
/// Absence is a failure rather than an empty file: the tests that read this
/// exist to catch the file being deleted.
fn repository_dockerignore() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits under the repository root");
    std::fs::read_to_string(root.join(DOCKERIGNORE))
        .expect("the repository has a .dockerignore at its root")
}

/// PSS-FR-26 (PSS-FR-28, PSS-FR-29): Cancel is read while the context is
/// packed, not only after it has been sent.
///
/// The Docker Engine answers a build with nothing at all until the whole
/// context has reached it, so a build's own cancellation check — which
/// reads what the connection streams — cannot run during the upload. On a
/// large project that is the longest part of a build, and a Cancel that is
/// noticed only at the end of it holds the project's one build slot for
/// exactly as long as doing nothing would.
#[test]
fn a_cancelled_build_stops_packing_its_context() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let dir = tempfile::tempdir().unwrap();
    let body = "x".repeat(16 * 1024);
    for i in 0..64 {
        write(dir.path(), &format!("file-{i:02}.txt"), &body);
    }

    // What the whole context weighs, to measure a stopped one against.
    let mut whole: Vec<u8> = Vec::new();
    pack_context(dir.path(), &mut whole).expect("the context packs");

    let drain = |rx: std::sync::mpsc::Receiver<Vec<u8>>| {
        std::thread::spawn(move || rx.iter().map(|chunk| chunk.len()).sum::<usize>())
    };

    // Cancelled before the first byte: nothing is read and nothing is sent.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(CONTEXT_CHUNKS_IN_FLIGHT);
    let reader = drain(rx);
    let always = || true;
    let mut writer = ChannelWriter::new(tx).stops_when(&always);
    let problem = pack_context(dir.path(), &mut writer).expect_err("a cancelled pack stops");
    drop(writer);
    assert!(problem.contains(CONTEXT_CANCELLED), "{problem}");
    assert_eq!(reader.join().unwrap(), 0, "nothing reached the connection");

    // Where to cancel, measured rather than guessed. The packer buffers into
    // `CONTEXT_CHUNK` and sends whole chunks, so a write count picked out of
    // the air can land before the first boundary and deliver nothing —
    // which would say nothing about whether the packer stopped. Counting
    // what a whole context takes and cancelling three quarters of the way
    // through puts the cancellation after several chunks have gone and well
    // before the end, whatever tar writes per file and whatever the chunk
    // size is.
    let whole_writes = {
        let mut counting = CountingWriter::default();
        pack_context(dir.path(), &mut counting).expect("the context packs");
        counting.writes
    };
    assert!(whole_writes > 8, "a context of 64 files takes more than 8 writes");

    // Cancelled part of the way through: the packer stops where it is
    // rather than finishing the project first.
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(CONTEXT_CHUNKS_IN_FLIGHT);
    let reader = drain(rx);
    let writes = AtomicUsize::new(0);
    let stop_at = whole_writes * 3 / 4;
    let partway = || writes.fetch_add(1, Ordering::SeqCst) >= stop_at;
    let mut writer = ChannelWriter::new(tx).stops_when(&partway);
    let problem = pack_context(dir.path(), &mut writer).expect_err("a cancelled pack stops");
    drop(writer);
    assert!(problem.contains(CONTEXT_CANCELLED), "{problem}");
    let sent = reader.join().unwrap();
    assert!(sent > 0, "the packer had started");
    assert!(sent < whole.len(), "it stopped before the end: {sent} of {}", whole.len());
}

/// A writer that keeps only the shape of what it was given: how many times
/// it was written to. It is what says where "part of the way through" is,
/// in the packer's own units rather than in a guess about them.
#[derive(Default)]
struct CountingWriter {
    writes: usize,
}

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writes += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A writer with no cancellation to read packs the whole project, which is
/// every caller that is not a build.
#[test]
fn a_writer_with_no_cancellation_packs_everything() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "Dockerfile", "FROM scratch\n");

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(CONTEXT_CHUNKS_IN_FLIGHT);
    let reader = std::thread::spawn(move || rx.iter().map(|c| c.len()).sum::<usize>());
    let mut writer = ChannelWriter::new(tx);
    pack_context(dir.path(), &mut writer).expect("the context packs");
    std::io::Write::flush(&mut writer).expect("the remainder is sent");
    drop(writer);
    assert!(reader.join().unwrap() > 0);
}

/// The bounded channel is what keeps a build of any project inside a fixed
/// amount of memory: the writer hands whole chunks on and holds only the
/// remainder.
#[test]
fn the_context_writer_hands_on_bounded_chunks() {
    use std::io::Write;

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(64);
    let mut writer = ChannelWriter::new(tx);
    // Two and a half chunks.
    let body = vec![7u8; CONTEXT_CHUNK * 2 + 11];
    writer.write_all(&body).unwrap();
    writer.flush().unwrap();
    drop(writer);

    let chunks: Vec<Vec<u8>> = rx.into_iter().collect();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].len(), CONTEXT_CHUNK);
    assert_eq!(chunks[1].len(), CONTEXT_CHUNK);
    assert_eq!(chunks[2].len(), 11);
    assert_eq!(
        chunks.iter().map(Vec::len).sum::<usize>(),
        body.len(),
        "every byte reached the connection"
    );
}

/// A reader that has gone — a cancelled build — ends the packer rather
/// than panicking it.
#[test]
fn a_dropped_reader_ends_the_packer() {
    use std::io::Write;

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
    drop(rx);
    let mut writer = ChannelWriter::new(tx);
    writer.write_all(&vec![0u8; CONTEXT_CHUNK]).unwrap_err();
}
