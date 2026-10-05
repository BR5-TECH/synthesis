//! Performing a project image build through the verified Docker backend
//! (`specifications/core/PSS-project-settings-storage.md` PSS-FR-26, PSS-FR-27).
//!
//! Two backends, one contract. In **Docker CLI** mode the verified executable is
//! run with `docker build --file <project-relative-Dockerfile> --tag
//! <image[:tag]> <project-root>` and its two pipes are read as they fill. In
//! **Bollard / Docker Engine** mode the same build runs over the Docker Engine
//! connection at the configured endpoint, whose image-build stream is read the
//! same way; the Docker CLI path takes no part in it.
//!
//! Neither path pushes, publishes, logs in to a registry, or writes anything
//! into the shipped vendor-image manifest. There is no code in this module that
//! reaches a registry at all, which is what makes that property inspectable
//! rather than remembered.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::images::{
    docker_build_args, read_build_line, BuildTerminal, ImageBuildRequest, ImageBuildSink,
    ImageBuilder, PHASE_BUILDING,
};
use crate::docker::{DockerEndpoint, ResolvedDockerBackend};

/// How often the CLI build's pipes are checked while nothing is arriving, so a
/// cancellation is acted on promptly without spinning.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// How long the Docker Engine is given to answer the build request itself.
///
/// This bounds the wait for the daemon's **first** answer, not the build: the
/// Engine sends no response header until it has read the whole build context
/// and its builder has produced a line, and everything the build streams after
/// that is read without a deadline of any kind. So the value has to cover a
/// context upload and a daemon start, which on a large project is minutes —
/// Bollard's own default of two minutes is a build that dies before it begins.
///
/// It is a backstop and not a policy: a build that is merely slow is ended by
/// the author through Cancel (PSS-FR-28), and this is what stops a daemon that
/// answers nothing at all from holding the project's one build slot forever.
///
/// Read only by the Docker Engine build, which is compiled out under
/// `cfg(test)` because it reaches a daemon.
#[cfg_attr(test, allow(dead_code))]
const ENGINE_BUILD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Production builder: dispatches on the backend the author verified.
pub struct RealImageBuilder;

impl ImageBuilder for RealImageBuilder {
    fn build(
        &self,
        request: &ImageBuildRequest,
        backend: &ResolvedDockerBackend,
        sink: &dyn ImageBuildSink,
    ) -> BuildTerminal {
        match backend {
            ResolvedDockerBackend::Cli { path } => build_with_cli(path, request, sink),
            ResolvedDockerBackend::Engine { endpoint } => {
                build_with_engine(endpoint, request, sink)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Docker CLI mode
// ---------------------------------------------------------------------------

/// PSS-FR-26: run the verified executable with the local-build vector, and read
/// its output as it arrives (PSS-FR-27).
fn build_with_cli(
    program: &str,
    request: &ImageBuildRequest,
    sink: &dyn ImageBuildSink,
) -> BuildTerminal {
    use std::process::{Command, Stdio};

    let args = docker_build_args(
        &request.dockerfile,
        &request.image_reference,
        &request.context_root,
    );
    let mut child = match Command::new(program)
        .args(&args)
        // The context is the project root, and `docker build` resolves
        // `--file` against the directory it is run in, so the root is both the
        // working directory and the context argument.
        .current_dir(&request.context_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return BuildTerminal::Failed(format!("the Docker CLI could not be run: {e}")),
    };

    // Both pipes are drained on threads: `docker build` writes its progress to
    // stderr and its result to stdout, and a build that filled one pipe while
    // this thread waited on the other would deadlock.
    let (lines_tx, lines_rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    for stream in [
        child.stdout.take().map(Pipe::Out),
        child.stderr.take().map(Pipe::Err),
    ]
    .into_iter()
    .flatten()
    {
        let tx = lines_tx.clone();
        readers.push(std::thread::spawn(move || stream.pump(tx)));
    }
    drop(lines_tx);

    let mut last_position: Option<(u64, u64)> = None;
    let mut tail: Vec<String> = Vec::new();
    let cancelled = loop {
        // Drain whatever has arrived without blocking on more.
        while let Ok(line) = lines_rx.try_recv() {
            report(sink, &line, &mut last_position, &mut tail);
        }
        if sink.cancelled() {
            let _ = child.kill();
            break true;
        }
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(e) => {
                let _ = child.kill();
                return BuildTerminal::Failed(format!("the Docker CLI could not be waited on: {e}"));
            }
        }
    };

    // Whatever the readers still hold, once the child is gone.
    for reader in readers {
        let _ = reader.join();
    }
    while let Ok(line) = lines_rx.try_recv() {
        report(sink, &line, &mut last_position, &mut tail);
    }

    let status = match child.wait() {
        Ok(status) => status,
        Err(e) => return BuildTerminal::Failed(format!("the Docker CLI could not be reaped: {e}")),
    };
    if cancelled {
        return BuildTerminal::Cancelled;
    }
    if status.success() {
        BuildTerminal::Succeeded
    } else {
        BuildTerminal::Failed(diagnostic_from(&tail, status.code()))
    }
}

enum Pipe {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

impl Pipe {
    /// Read the pipe line by line and hand each on. Lossy on purpose: a build
    /// that emitted one invalid byte must still report the rest of what it said.
    fn pump(self, tx: std::sync::mpsc::Sender<String>) {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut read = |target: &mut dyn Read| loop {
            match target.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buffer.extend_from_slice(&chunk[..n]);
                    while let Some(at) = buffer.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = buffer.drain(..=at).collect();
                        if tx
                            .send(String::from_utf8_lossy(&line).to_string())
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Err(_) => break,
            }
        };
        match self {
            Pipe::Out(mut out) => read(&mut out),
            Pipe::Err(mut err) => read(&mut err),
        }
        if !buffer.is_empty() {
            let _ = tx.send(String::from_utf8_lossy(&buffer).to_string());
        }
    }
}

/// How many trailing lines a failure diagnostic keeps.
const DIAGNOSTIC_LINES: usize = 12;

/// PSS-FR-28: what a failure says.
///
/// Docker's own build output and nothing else: no credential and no registry
/// secret can be in it, because nothing in this module presents one — and the
/// tail is bounded so a build that failed after ten thousand lines still gives
/// the author something they can read.
fn diagnostic_from(tail: &[String], code: Option<i32>) -> String {
    let body: Vec<&str> = tail
        .iter()
        .rev()
        .take(DIAGNOSTIC_LINES)
        .rev()
        .map(String::as_str)
        .filter(|line| !line.is_empty())
        .collect();
    if body.is_empty() {
        match code {
            Some(code) => format!("the build failed with exit status {code}"),
            None => "the build failed without an exit status".to_string(),
        }
    } else {
        body.join("\n")
    }
}

/// Publish one line of the backend's output, and remember it in case the build
/// fails (PSS-FR-27, PSS-FR-28).
fn report(
    sink: &dyn ImageBuildSink,
    line: &str,
    last_position: &mut Option<(u64, u64)>,
    tail: &mut Vec<String>,
) {
    let (position, message) = read_build_line(line);
    if message.is_empty() {
        return;
    }
    if let Some(position) = position {
        // PSS-FR-27: an operation may become determinate once the backend
        // reports a total, and never returns to indeterminate afterwards.
        *last_position = Some(position);
    }
    tail.push(message.clone());
    if tail.len() > DIAGNOSTIC_LINES * 4 {
        tail.drain(..DIAGNOSTIC_LINES);
    }
    let (completed, total) = match last_position {
        Some((done, total)) => (Some(*done), Some(*total)),
        None => (None, None),
    };
    sink.progress(PHASE_BUILDING, completed, total, &message);
}

// ---------------------------------------------------------------------------
// Bollard / Docker Engine mode
// ---------------------------------------------------------------------------

/// PSS-FR-26: build over the Docker Engine connection at the configured
/// endpoint, reading its image-build stream.
#[cfg(not(test))]
fn build_with_engine(
    endpoint: &DockerEndpoint,
    request: &ImageBuildRequest,
    sink: &dyn ImageBuildSink,
) -> BuildTerminal {
    // The Engine API takes the build context as a tar stream where the CLI
    // takes a directory, so the project root is packed here. The author is
    // told, because a large project makes this the slowest part of a small
    // build and a section reporting nothing during it looks stalled.
    //
    // The packing runs on its own thread and the archive is never held whole:
    // it reaches the connection a chunk at a time through a bounded channel, so
    // a project of any size costs the same fixed amount of memory. A failure to
    // read the project is reported by the thread rather than raised here,
    // because the connection has already been handed the receiving end.
    sink.progress(
        super::images::PHASE_CONTEXT,
        None,
        None,
        "packing the build context",
    );
    if sink.cancelled() {
        return BuildTerminal::Cancelled;
    }
    let (chunks_tx, chunks_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(CONTEXT_CHUNKS_IN_FLIGHT);
    let (packed_tx, packed_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let context_root = request.context_root.clone();
    // A scope rather than a bare spawn, so the packing thread can read the
    // build's own cancellation. The scope ends only after that thread has, so
    // the packer never outlives the build it packs for.
    std::thread::scope(|threads| {
        threads.spawn(|| {
            let cancelled = || sink.cancelled();
            let mut writer = ChannelWriter::new(chunks_tx).stops_when(&cancelled);
            let outcome = pack_context(&context_root, &mut writer)
                .and_then(|()| std::io::Write::flush(&mut writer).map_err(|e| e.to_string()));
            let _ = packed_tx.send(outcome);
        });
        build_over_connection(endpoint, request, sink, chunks_rx, packed_rx)
    })
}

/// The half of an Engine build that talks to the daemon, with the packer
/// already running on a thread beside it.
#[cfg(not(test))]
fn build_over_connection(
    endpoint: &DockerEndpoint,
    request: &ImageBuildRequest,
    sink: &dyn ImageBuildSink,
    chunks_rx: std::sync::mpsc::Receiver<Vec<u8>>,
    packed_rx: std::sync::mpsc::Receiver<Result<(), String>>,
) -> BuildTerminal {
    use futures_util::StreamExt;

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return BuildTerminal::Failed(format!("the build could not be started: {e}")),
    };

    runtime.block_on(async move {
        let docker = match crate::docker::connect(endpoint, ENGINE_BUILD_TIMEOUT) {
            Ok(docker) => docker,
            Err(e) => {
                // GSS-FR-40: the backend verified once and is unavailable now.
                // The build reports it rather than a second verification.
                return BuildTerminal::Failed(format!("the Docker Engine could not be reached: {e}"));
            }
        };
        let options = bollard::query_parameters::BuildImageOptionsBuilder::default()
            .dockerfile(&request.dockerfile)
            .t(&request.image_reference)
            // Intermediate containers are removed, exactly as `docker build`
            // does by default. Nothing here names a registry, a credential, or
            // an output that would publish: the build lands in this machine's
            // own image store and nowhere else (PSS-FR-26).
            .rm(true)
            .build();
        // The body, one chunk at a time. `recv` is a blocking call, so it is
        // made on the blocking pool rather than on the runtime's own thread —
        // a body that blocked the executor would stop the build's own output
        // being read at the same time.
        let chunks = std::sync::Arc::new(std::sync::Mutex::new(chunks_rx));
        let body = futures_util::stream::unfold(chunks, |chunks| async move {
            let handle = std::sync::Arc::clone(&chunks);
            let next = tokio::task::spawn_blocking(move || {
                handle.lock().ok().and_then(|rx| rx.recv().ok())
            })
            .await
            .ok()
            .flatten()?;
            Some((next.into(), chunks))
        });
        let mut stream = docker.build_image(
            options,
            // No credentials: this build authenticates to no registry, because
            // it reaches none.
            None,
            Some(bollard::body_stream(body)),
        );

        let mut last_position: Option<(u64, u64)> = None;
        let mut tail: Vec<String> = Vec::new();
        let mut failure: Option<String> = None;
        while let Some(item) = stream.next().await {
            if sink.cancelled() {
                return BuildTerminal::Cancelled;
            }
            match item {
                Ok(info) => {
                    if let Some(detail) = info.error_detail.as_ref() {
                        failure = Some(
                            detail
                                .message
                                .clone()
                                .unwrap_or_else(|| "the build failed".to_string()),
                        );
                    }
                    // `stream` carries the same lines the CLI prints; `status`
                    // carries the pull-style progress of a base image.
                    for text in [info.stream.as_deref(), info.status.as_deref()]
                        .into_iter()
                        .flatten()
                    {
                        for line in text.split_inclusive('\n') {
                            report(sink, line, &mut last_position, &mut tail);
                        }
                    }
                    if let Some(progress) = info.progress_detail.as_ref() {
                        if let (Some(current), Some(total)) = (progress.current, progress.total) {
                            if total > 0 && current >= 0 {
                                last_position = Some((current as u64, total as u64));
                            }
                        }
                    }
                }
                Err(e) => {
                    failure = Some(e.to_string());
                    break;
                }
            }
        }
        // PSS-FR-28: a cancelled build is cancelled, whatever the connection
        // then said. Stopping the packer ends the request body early, so the
        // daemon reports a truncated archive — which is a symptom of the
        // cancellation and never a result to report in place of it.
        if sink.cancelled() {
            return BuildTerminal::Cancelled;
        }
        // A project the packer could not read is the build's failure, and it is
        // the one the author can act on — the daemon's own "unexpected EOF" is
        // a symptom of it rather than the cause.
        if let Ok(Err(problem)) = packed_rx.try_recv() {
            return BuildTerminal::Failed(format!(
                "the build context could not be read: {problem}"
            ));
        }
        match failure {
            None => BuildTerminal::Succeeded,
            Some(message) => {
                tail.push(message);
                BuildTerminal::Failed(diagnostic_from(&tail, None))
            }
        }
    })
}

/// Under `cfg(test)` the production engine build reaches nothing: the suite must
/// run identically with and without a Docker daemon, so a test that exercises a
/// build binds a scripted [`ImageBuilder`] rather than this one.
#[cfg(test)]
fn build_with_engine(
    _endpoint: &DockerEndpoint,
    _request: &ImageBuildRequest,
    _sink: &dyn ImageBuildSink,
) -> BuildTerminal {
    BuildTerminal::Failed("the production engine is not reachable under test".to_string())
}

// ---------------------------------------------------------------------------
// The build context (PSS-FR-26)
// ---------------------------------------------------------------------------

/// How much of the archive is held in memory at once, and how large a piece the
/// Engine is handed at a time.
const CONTEXT_CHUNK: usize = 256 * 1024;

/// How many chunks may sit between the packer and the connection.
///
/// The bound is the whole point: the packing thread blocks once this many are
/// waiting, so a project of any size costs `CONTEXT_CHUNK * CONTEXT_CHUNKS_IN_FLIGHT`
/// of memory rather than its own weight in it.
///
/// Read only by the Docker Engine build, which is compiled out under `cfg(test)`
/// because it reaches a daemon; the packing it bounds is exercised directly.
#[cfg_attr(test, allow(dead_code))]
const CONTEXT_CHUNKS_IN_FLIGHT: usize = 4;

/// What the packer reports when a cancellation ended it.
///
/// It never reaches the author: `build_with_engine` answers a cancelled build
/// with `Cancelled` (PSS-FR-28), and this only tells the two apart from a
/// project the packer could not read.
const CONTEXT_CANCELLED: &str = "the build was cancelled while its context was packed";

/// The names that are never part of a build context.
///
/// `.git` is excluded by decision rather than by omission: no Dockerfile can
/// usefully reference an object store, and sending one is the difference
/// between a build that takes a second and one that takes a minute.
const ALWAYS_EXCLUDED: &[&str] = &[".git"];

/// The file whose patterns say what else is left out.
pub const DOCKERIGNORE: &str = ".dockerignore";

/// A `.dockerignore`, read into the patterns it holds.
///
/// A **subset** of Docker's own matcher, and deliberately a documented one: a
/// path segment matches literally or through `*` and `?`, a `**` segment matches
/// any run of segments, a pattern with no `**` also excludes everything beneath
/// what it names, and a `!` prefix re-includes. What is not supported is
/// Go's character-class syntax, which no ordinary ignore file uses and which a
/// half-implementation of would silently mis-exclude.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DockerIgnore {
    rules: Vec<IgnoreRule>,
}

#[derive(Debug, PartialEq, Eq)]
struct IgnoreRule {
    /// `true` for a `!` rule, which re-includes what an earlier rule excluded.
    negated: bool,
    segments: Vec<String>,
}

impl DockerIgnore {
    /// Read the patterns out of a `.dockerignore`'s text.
    pub fn parse(text: &str) -> DockerIgnore {
        let mut rules = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            // A comment and a blank line are both nothing to match.
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (negated, pattern) = match line.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, line),
            };
            let pattern = pattern.trim_start_matches("./").trim_matches('/');
            if pattern.is_empty() {
                continue;
            }
            rules.push(IgnoreRule {
                negated,
                segments: pattern.split('/').map(str::to_string).collect(),
            });
        }
        DockerIgnore { rules }
    }

    /// Is this project-relative path excluded?
    ///
    /// The last rule that matches decides, which is how a `!` re-include takes
    /// effect over the exclusion above it.
    pub fn excludes(&self, relative: &Path) -> bool {
        let path: Vec<String> = relative
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(name) => {
                    Some(name.to_string_lossy().into_owned())
                }
                _ => None,
            })
            .collect();
        if path.is_empty() {
            return false;
        }
        let mut excluded = false;
        for rule in &self.rules {
            if matches_pattern(&rule.segments, &path) {
                excluded = !rule.negated;
            }
        }
        excluded
    }

    /// Could anything **beneath** this directory be re-included?
    ///
    /// An excluded directory is normally not descended into at all. A `!` rule
    /// naming something inside it is the one case where it must be, or the
    /// re-include could never take effect.
    pub fn may_reinclude_below(&self, relative: &Path) -> bool {
        let depth = relative
            .components()
            .filter(|c| matches!(c, std::path::Component::Normal(_)))
            .count();
        self.rules
            .iter()
            .any(|rule| rule.negated && (rule.segments.len() > depth || rule.segments.iter().any(|s| s == "**")))
    }
}

/// Does a pattern's segments match a path's, with `**` spanning any run of them?
fn matches_pattern(pattern: &[String], path: &[String]) -> bool {
    match pattern.split_first() {
        // The pattern ran out. It matches this path, and — because a Docker
        // ignore pattern excludes what it names together with everything under
        // it — every path beneath it too.
        None => true,
        Some((head, rest)) if head == "**" => {
            // `**` takes nothing, then one segment, then two, and so on.
            (0..=path.len()).any(|skip| matches_pattern(rest, &path[skip..]))
        }
        Some((head, rest)) => match path.split_first() {
            None => false,
            Some((segment, tail)) => {
                matches_segment(head, segment) && matches_pattern(rest, tail)
            }
        },
    }
}

/// One segment against one name, with `*` and `?` as Docker spells them.
fn matches_segment(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    // The classic two-pointer glob, which needs no allocation and no recursion.
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ni;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Pack the build context — the project root — into `writer` as a tar archive.
///
/// Every byte of the project is read through the governed instance rather than
/// by a bare directory walk, so the context is subject to the same allowlist
/// every other read in the application is (`FSA-filesystem-access.md`). Nothing
/// is buffered whole: the writer is handed each piece as the archive is built,
/// and what the writer does with it decides how much memory a build costs.
pub fn pack_context<W: std::io::Write>(root: &Path, writer: W) -> Result<(), String> {
    let access = crate::fs::RootFs::new(
        root.to_path_buf(),
        std::sync::Arc::new(
            crate::fs::FsAccess::builder()
                .allow_root(root)
                .build()
                .map_err(|e| e.to_string())?,
        ),
    );
    // The project's own `.dockerignore`, where it has one. Read through the
    // same instance; an unreadable one is no patterns rather than a failure,
    // because a build context is not the place to refuse a project over a file
    // Docker itself would ignore.
    let ignore = access
        .read_text(Path::new(DOCKERIGNORE))
        .map(|text| DockerIgnore::parse(&text))
        .unwrap_or_default();
    let probe = crate::agentic::RealFileProbe;
    let mut archive = tar::Builder::new(writer);
    append_dir(&access, &ignore, &probe, &mut archive, Path::new(""))?;
    archive.finish().map_err(|e| e.to_string())
}

/// Append one directory's entries, recursing into the ones the context keeps.
pub fn append_dir<W: std::io::Write>(
    access: &crate::fs::RootFs,
    ignore: &DockerIgnore,
    probe: &dyn crate::agentic::FileProbe,
    archive: &mut tar::Builder<W>,
    relative: &Path,
) -> Result<(), String> {
    let entries = access
        .access()
        .list_dir(access.path().join(relative))
        .map_err(|e| e.to_string())?;
    for entry in entries {
        let child = relative.join(&entry.name);
        if ALWAYS_EXCLUDED.contains(&entry.name.as_str()) {
            continue;
        }
        let excluded = ignore.excludes(&child);
        match entry.kind {
            crate::fs::EntryKind::Dir => {
                // An excluded directory is descended into only where a `!` rule
                // could re-include something beneath it.
                if excluded && !ignore.may_reinclude_below(&child) {
                    continue;
                }
                append_dir(access, ignore, probe, archive, &child)?;
            }
            crate::fs::EntryKind::File => {
                if excluded {
                    continue;
                }
                let bytes = access.read_bytes(&child).map_err(|e| e.to_string())?;
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                // The file's own executable bit, so a Dockerfile that copies a
                // script and runs it works here exactly as it does when the
                // Docker CLI sends the same directory.
                header.set_mode(if probe.is_executable(&access.path().join(&child)) {
                    0o755
                } else {
                    0o644
                });
                header.set_cksum();
                archive
                    .append_data(&mut header, &child, bytes.as_slice())
                    .map_err(|e| e.to_string())?;
            }
            // A symlink in a build context is followed by neither backend in a
            // way this application can reproduce faithfully, so it is left out
            // rather than packed as the file it points at. A FIFO, socket, or
            // device cannot be packed and its read could block, so it is left
            // out too.
            crate::fs::EntryKind::Symlink | crate::fs::EntryKind::Other => {}
        }
    }
    Ok(())
}

/// The `std::io::Write` the packer writes the archive into: a bounded channel
/// the Docker Engine connection reads its request body from.
///
/// Blocking on a full channel is the backpressure that keeps a build of any
/// project inside [`CONTEXT_CHUNK`] × [`CONTEXT_CHUNKS_IN_FLIGHT`] bytes. It is
/// safe to block here because the packer owns its own thread — the runtime
/// never waits on this writer.
struct ChannelWriter<'a> {
    tx: std::sync::mpsc::SyncSender<Vec<u8>>,
    pending: Vec<u8>,
    /// What the writer asks before it packs more, where a caller gave it
    /// something to ask. See [`ChannelWriter::stops_when`].
    cancelled: Option<&'a (dyn Fn() -> bool + Sync)>,
}

impl<'a> ChannelWriter<'a> {
    fn new(tx: std::sync::mpsc::SyncSender<Vec<u8>>) -> Self {
        ChannelWriter {
            tx,
            pending: Vec::with_capacity(CONTEXT_CHUNK),
            cancelled: None,
        }
    }

    /// PSS-FR-28: give the writer the cancellation to read.
    ///
    /// The packer is the one part of an Engine build that runs before the
    /// daemon has said anything, and the daemon says nothing until the whole
    /// context has reached it. So the build's own cancellation check — which
    /// reads what the connection streams — cannot run yet, and this is where a
    /// cancellation is noticed while a large project is being packed. Without
    /// it the author's Cancel is read only once the upload has finished, and
    /// the project's one build slot (PSS-FR-29) is held until then.
    ///
    /// Asked on every write rather than every chunk, so the answer is acted on
    /// within one file rather than within 256 KB.
    fn stops_when(mut self, cancelled: &'a (dyn Fn() -> bool + Sync)) -> Self {
        self.cancelled = Some(cancelled);
        self
    }

    /// Has the build been cancelled since the last piece was packed?
    fn stopped(&self) -> bool {
        self.cancelled.is_some_and(|ask| ask())
    }

    fn send(&mut self, chunk: Vec<u8>) -> std::io::Result<()> {
        self.tx.send(chunk).map_err(|_| {
            // The reader is gone — the build was cancelled or the connection
            // failed. Ending the packer is the right answer, not a panic.
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the build context is no longer read")
        })
    }
}

/// PSS-FR-28: a cancelled build stops, and stopping is **terminal**.
///
/// The kind is load-bearing rather than decoration. `ErrorKind::Interrupted`
/// means "this can be retried" to every caller in the standard library, and
/// `write_all` and `io::copy` are required to ignore it and write again — so a
/// packer that reported a cancellation that way would be asked to write, refuse,
/// and be asked again for as long as the cancellation stood, holding the
/// project's one build slot and a whole core while doing it. Any other kind ends
/// the write, which is what a cancellation is for (PSS-FR-28, PSS-FR-29: nothing retries on
/// its own).
fn cancelled() -> std::io::Error {
    std::io::Error::other(CONTEXT_CANCELLED)
}

impl std::io::Write for ChannelWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.stopped() {
            return Err(cancelled());
        }
        self.pending.extend_from_slice(buf);
        while self.pending.len() >= CONTEXT_CHUNK {
            let rest = self.pending.split_off(CONTEXT_CHUNK);
            let chunk = std::mem::replace(&mut self.pending, rest);
            self.send(chunk)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.stopped() {
            return Err(cancelled());
        }
        if !self.pending.is_empty() {
            let chunk = std::mem::take(&mut self.pending);
            self.send(chunk)?;
        }
        Ok(())
    }
}

/// The cancellation flag a running build reads, wrapped as the sink's own
/// question.
pub struct CancelFlag(pub std::sync::Arc<std::sync::atomic::AtomicBool>);

impl CancelFlag {
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests;
