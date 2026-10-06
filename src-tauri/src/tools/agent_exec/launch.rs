//! The launch itself: one call to `execute_agent_cli` from the point the task
//! document is read to the point a normalized result is returned.
//!
//! Split out of `agent_exec.rs` for size alone. Everything here is a method of
//! [`AgentCliExecutor`] and reads the module's own private items through
//! `super`.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use super::*;

impl AgentCliExecutor {
    pub(super) async fn launch<S>(
        &self,
        sink: &S,
        context: &LaunchContext<'_>,
        request: AgentExecutionRequest,
        started: Instant,
    ) -> Result<AgentExecution, AgentExecutionError>
    where
        S: LogSink + Clone + Send + Sync + 'static,
    {
        // --- 1. The task document, before anything external is touched ------
        request
            .task
            .validate()
            .map_err(AgentExecutionError::TaskInvalid)?;
        let stdin_bytes = request
            .task
            .to_stdin_bytes()
            .map_err(AgentExecutionError::RequestTooLarge)?;

        // --- 2. The integration, resolved by the executor itself ------------
        //
        // EAC-FR-03: the executor calls AIC, never the caller. A caller cannot
        // pre-resolve and hand in a vendor, because there is no field for one.
        let invocation =
            agentic::resolve_agentic_invocation(
                context.store,
                context.integrations,
                context.project_key,
                // EAC-FR-IRRD: which of the author's own model and effort
                // selections apply to this turn. The kind is the whole of what
                // the caller says about either; both are read here from the
                // resolved integration.
                Some(request.turn_kind.selection_key()),
            )
                .map_err(AgentExecutionError::IntegrationUnresolved)?;

        let (vendor, model_id, effort_id) = match invocation {
            AgenticInvocation::Cli {
                vendor,
                model_id,
                effort_id,
                // EAC-FR-04 note: `binary_path` is deliberately dropped here. A
                // stored path is AIC's verification and detection datum
                // (AIC-FR-05); the executor runs a pinned image rather than
                // whatever happens to be installed on this machine.
                ..
            } => (vendor, model_id, effort_id),
            AgenticInvocation::Api { vendor, .. } => {
                return Err(AgentExecutionError::UnsupportedIntegration(vendor));
            }
        };

        let descriptor = descriptor::descriptor_for(&vendor)
            .ok_or_else(|| AgentExecutionError::UnsupportedIntegration(vendor.clone()))?;
        // EAC-FR-38: the image is the one the open project commits for this
        // vendor, used directly. There is no fallback to a shipped vendor image
        // and no manifest digest to pin by, so a project that configures none
        // launches nothing — before a credential is requested and before any
        // container is created.
        let image = context
            .images
            .image_for(&vendor)
            .map_err(AgentExecutionError::VendorImageUnresolved)?;

        // CCP-FR-05: an effort identifier the pinned CLI would refuse is dropped
        // here rather than generated into a vector. Passing it through would
        // create a container that the CLI then rejects for an argument the
        // application chose, which costs a launch to learn nothing.
        let effort_id = effort_id.filter(|effort| {
            let supported = descriptor.effort_supported(effort);
            if !supported {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &[Domain::Ai, Domain::Backend],
                    "resolved reasoning effort is not one the pinned CLI accepts",
                    log_fields! {
                        "vendor" => vendor.as_str(),
                        "effort" => effort.as_str(),
                    },
                );
            }
            supported
        });

        // --- 3. Resume, before anything is spent on a turn that cannot run ---
        let resume_session = request
            .task
            .resume
            .as_ref()
            .and_then(|r| r.session_id.as_deref())
            .filter(|id| !id.is_empty());
        if resume_session.is_some() && !descriptor.supports_resume() {
            // EAC-FR-30: told, not silently started fresh. A caller that meant
            // to continue a session would otherwise get a turn with no memory
            // of the one before it and no way to know.
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "agent session resumption unavailable",
                log_fields! { "vendor" => vendor.as_str() },
            );
            return Ok(AgentExecution {
                process_outcome: ProcessOutcome::Terminated,
                exit_code: None,
                response: None,
                stdout: CapturedStream::default(),
                stderr: CapturedStream::default(),
                duration_ms: started.elapsed().as_millis() as u64,
                resume_unavailable: true,
                session: None,
                // No container was ever created, so there is nothing to remove
                // and nothing left behind.
                container_removed: true,
            });
        }

        // --- 4. The execution directory -------------------------------------
        let execution_directory = self.validate_directory(context.fs, &request.execution_directory)?;
        let mount_source = descriptor::mount_source(&execution_directory).ok_or(
            AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Unmountable),
        )?;

        // --- 5. Launch material, from AIC and nowhere else -------------------
        let credential =
            agentic::resolve_agent_launch_credential(context.integrations, &vendor)
                .map_err(AgentExecutionError::CredentialUnavailable)?;

        // EAC-FR-ZKMR: the execution directory alone decides the shape. Where
        // its host path is one a container can be given, the container names it
        // and the application and the turn inside name one string for one file;
        // where it is not, the launch takes the fallback shape and the directory
        // stands at the fixed path.
        let aligned = descriptor::container_path(&execution_directory);
        let workspace = aligned
            .clone()
            .unwrap_or_else(|| WORKSPACE_TARGET.to_string());
        let mut mounts = vec![BindMount {
            source: mount_source.clone(),
            target: workspace.clone(),
            read_only: false,
        }];
        let mut env: BTreeMap<String, SecretString> = BTreeMap::new();
        let mut env_names: Vec<&str> = Vec::new();
        let mut env_literals: Vec<(String, String)> = Vec::new();
        // EAC-FR-29 bars the Codex configuration path from a record as firmly as
        // it bars Claude's token, and this vendor's credential is a directory
        // rather than an environment value — so the path is what has to be
        // masked out of a stderr excerpt, and it is kept here for that.
        let mut masked_paths: Vec<String> = Vec::new();

        match credential {
            AgentLaunchCredential::ClaudeOauthToken(token) => {
                // EAC-FR-15: the *name* goes in the argv, the value goes in the
                // client process's environment, and Docker forwards it. The
                // secret never reaches an argument vector.
                env_names.push(CLAUDE_TOKEN_ENV);
                env.insert(CLAUDE_TOKEN_ENV.to_string(), token);
            }
            AgentLaunchCredential::CodexConfigMount { source, target } => {
                let source = descriptor::mount_source(&source).ok_or(
                    AgentExecutionError::CredentialUnavailable(
                        agentic::ERR_CODEX_CONFIG_MISSING.to_string(),
                    ),
                )?;
                let target = target.to_string_lossy().into_owned();
                // EAC-FR-16 / CDX-FR-23: read/write, because this vendor writes
                // its session rollout files beneath the same directory it reads
                // its login from. A read-only mount leaves it unable to persist
                // a session — which makes EAC-FR-30's resume unreachable — and
                // unable to refresh a credential that has expired. Nothing here
                // reads, copies, or logs a byte of it either way.
                // Both spellings: the CLI names its own directory by the
                // container path, and a Docker diagnostic names it by the host
                // path it was mounted from.
                masked_paths.push(source.clone());
                masked_paths.push(target.clone());
                mounts.push(BindMount {
                    source,
                    target: target.clone(),
                    read_only: false,
                });
                // CDX-FR-24: the same directory, named to the CLI.
                if let descriptor::SessionStateMount::SharedWithCredentialMount { env: name } =
                    descriptor.session_state
                {
                    env_literals.push((name.to_string(), target));
                }
            }
        }

        // EAC-FR-31: a vendor whose session directory is its own gets a
        // dedicated mount. Without it every session dies with the container that
        // created it and a later resume has nothing to continue.
        // EAC-FR-40: held beyond this block, because the supplementary mount is
        // validated against it — a bundle overlapping the session-state
        // directory would sit behind a mount the container may **write** to.
        let mut session_state_dir: Option<PathBuf> = None;
        if let Some((host_dir, env_name, target)) =
            self.session_state_for(context.fs, descriptor, context.project_key)?
        {
            let source = descriptor::mount_source(&host_dir).ok_or(
                AgentExecutionError::ExecutionDirectoryInvalid(DirectoryProblem::Unmountable),
            )?;
            mounts.push(BindMount {
                source,
                target: target.to_string(),
                read_only: false,
            });
            env_literals.push((env_name.to_string(), target.to_string()));
            session_state_dir = Some(host_dir);
        }

        // --- 5b. EAC-FR-40: the one typed supplementary mount ----------------
        //
        // Validated before any container is created, and every check refuses
        // with `SupplementaryMountInvalid` naming which. Nothing here reads a
        // byte inside the bundle: its contents are the caller's material.
        if let Some(mount) = request.supplementary_mount.as_ref() {
            let host = mount.host_path();
            validate_supplementary_mount(
                context.fs,
                &self.data_roots()?,
                host,
                &mount_source,
                session_state_dir.as_deref(),
                &masked_paths,
            )?;
            let source = descriptor::mount_source(host).ok_or_else(|| {
                AgentExecutionError::SupplementaryMountInvalid("unmountable path".to_string())
            })?;
            mounts.push(BindMount {
                source,
                target: mount.container_root().to_string(),
                // Read-only, always. Not a caller's to choose.
                read_only: true,
            });
        }

        // --- 5c. EAC-FR-41: repository metadata, masked ----------------------
        //
        // Stated as a launch rule rather than left to follow from the mount
        // set, because the mount set alone does not carry it: a linked
        // worktree's `.git` file happens to name a gitdir outside every mount,
        // a repository whose metadata sits inside the mounted tree does not,
        // and a nested checkout the caller's own work created does not either.
        // A guarantee that holds only while three unrelated things stay true is
        // one nobody can rely on.
        // EAC-FR-41: keyed on **either** condition and not on one alone, so a
        // turn that reached here under the wrong name still cannot read a
        // repository. Masking and the read-only access below are exclusive, and
        // masking wins wherever both would apply.
        let masked = request.supplementary_mount.is_some()
            || request.turn_kind == TurnKind::SemanticRebase;
        if masked {
            let mask = self.empty_mask_dir(context.fs)?;
            for (rel, kind) in
                repository_metadata_paths(context.fs, &request.execution_directory)?
            {
                // EAC-FR-41: masked by something of its own kind, or the kernel
                // refuses the mount and the repository stays reachable.
                let source = descriptor::mount_source(mask.source_for(kind)).ok_or_else(|| {
                    AgentExecutionError::RepositoryMaskingUnavailable(
                        "the mask cannot be mounted".to_string(),
                    )
                })?;
                mounts.push(BindMount {
                    source,
                    target: format!("{workspace}/{rel}"),
                    read_only: true,
                });
            }
        }

        // --- 5d. EAC-FR-FNFV: repository metadata, mounted read-only ---------
        //
        // A turn that revises or judges a change set has to know what changed in
        // it, and the only alternative to asking Git is reading every file of
        // the working copy whole — which costs more than the judgement and grows
        // with the size of the project rather than with the size of the change.
        //
        // Best-effort by contract: a directory that belongs to no repository, or
        // metadata that cannot be mounted, launches **without** this and returns
        // no error. Repository access is a convenience of a turn and never a
        // precondition of one, which is the opposite of the masking above.
        if !masked {
            match self.repository_mounts(context.fs, &execution_directory, &workspace, aligned.is_some())
            {
                Some((repository, env, linked)) => {
                    mounts.extend(repository);
                    env_literals.extend(env);
                    crate::logging::log_debug(
                        sink,
                        &crate::logging::BUFFER,
                        // The same two domains every other record of a launch
                        // carries, so neither of the panel's filters misses it.
                        &[crate::logging::Domain::Ai, crate::logging::Domain::Backend],
                        "agent execution reaches its repository read-only",
                        crate::log_fields! {
                            "turn_kind" => request.turn_kind.as_str(),
                            "linked_worktree" => linked,
                        },
                    );
                }
                // The handled degradation, recorded rather than silent. A turn
                // that read every file whole because it could not reach Git is
                // otherwise a mystery to whoever reads the log afterwards.
                None => crate::logging::log_debug(
                    sink,
                    &crate::logging::BUFFER,
                    &[crate::logging::Domain::Ai, crate::logging::Domain::Backend],
                    "agent execution reaches no repository and reads files instead",
                    crate::log_fields! { "turn_kind" => request.turn_kind.as_str() },
                ),
            }
        }

        // --- 6. The runtime --------------------------------------------------
        let runtime = self.runtime_for(context.store)?;
        if runtime.ensure_available().await.is_err() {
            // EAC-FR-37: what was run and where it was looked for. Without it a
            // machine whose runtime is installed and running reports having
            // none, and the author is left to guess at the difference between
            // an absent runtime, a stopped one, and one this process cannot
            // reach — three different corrections behind one sentence.
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Backend],
                "no container runtime answered",
                log_fields! {
                    "program" => runtime.resolved_program().unwrap_or_default(),
                    "searched" => descriptor::docker_search_paths(),
                    "path" => std::env::var("PATH").unwrap_or_default(),
                },
            );
            return Err(AgentExecutionError::RuntimeUnavailable);
        }

        // EAC-FR-38: where the configured image is absent from the local image
        // store the backend may ask Docker to pull that reference, and a pull
        // that cannot find or fetch it is `ImageUnavailable`. `ensure_image`
        // inspects before it pulls, so an image the author built locally runs
        // without a registry ever being reached.
        runtime
            .ensure_image(&image)
            .await
            .map_err(|error| match error {
                RuntimeError::ImageUnavailable => {
                    AgentExecutionError::ImageUnavailable(vendor.clone())
                }
                RuntimeError::Unavailable => AgentExecutionError::RuntimeUnavailable,
                RuntimeError::LaunchFailed(reason) => AgentExecutionError::LaunchFailed(reason),
            })?;

        // --- 7. The invocation ------------------------------------------------
        let (host_uid, host_gid) = descriptor::host_ids();

        // EAC-FR-21: a vendor that lets the executor name the session is given
        // one before it runs, so the turn's identity is known rather than
        // recovered afterwards. A vendor that only reports its own gets none,
        // and the value below is never placed in its vector.
        let assigned_session_id = descriptor::new_session_id();

        let vendor_args = descriptor.vendor_args(
            model_id.as_deref(),
            effort_id.as_deref(),
            &assigned_session_id,
            resume_session,
            // EAC-FR-43: the shape the caller named for its own answer, which
            // the vendor enforces where its CLI can.
            request.task.result_contract,
        );
        let container_name = descriptor::unique_container_name();
        // EAC-FR-10 / EAC-FR-39: one specification, composed from the
        // descriptor, the named constants, and the project's image. The Docker
        // CLI backend renders it into the `docker run` vector below; the Docker
        // Engine backend creates and starts the same specification over its own
        // connection.
        let container = descriptor::ContainerSpec {
            name: container_name.clone(),
            image: image.clone(),
            host_uid,
            host_gid,
            workdir: workspace.clone(),
            mounts: mounts.clone(),
            env_names: env_names.iter().map(|n| (*n).to_string()).collect(),
            env_literals: env_literals.clone(),
            vendor_args: vendor_args.clone(),
        };
        let argv = container.docker_run_args();

        // EAC-FR-29: everything this launch handed the container that a record
        // may not carry — the secrets in its environment, the credential paths
        // that are a credential's location rather than its value, and the
        // session identities. Assembled once and used by every masked thing
        // below it, so the observer and the failure excerpt cannot come to
        // disagree about what a record may say.
        let secrets: Vec<String> = env
            .values()
            .map(|value| value.expose().to_string())
            .chain(masked_paths.iter().cloned())
            .chain(std::iter::once(assigned_session_id.clone()))
            .chain(resume_session.map(str::to_string))
            .collect();

        logging::log_info(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "agent execution starting",
            log_fields! {
                "vendor" => vendor.as_str(),
                // Which container this turn is in, so a record can be lined up
                // against `docker ps` while it runs and against a leftover
                // afterwards. Generated, and not sensitive.
                "container" => container_name.as_str(),
                "resumed" => resume_session.is_some(),
                // EAC-FR-ZKMR: which shape this launch took. Every path a turn
                // reports means one thing in the aligned shape and another in
                // the fallback one, so a reader who does not know which is
                // reading those paths without knowing what they name. A boolean
                // of this module's own, and not a path.
                "paths_aligned" => aligned.is_some(),
                // EAC-FR-43: which shape this turn's answer is held to, so a
                // reader can tell an answer the vendor validated from one
                // nobody did. An identifier of a closed set, and not sensitive.
                "result_contract" => request
                    .task
                    .result_contract
                    .map(|contract| contract.as_str().to_string()),
                "timeout_ms" => request.task.execution.timeout_ms,
                "task_bytes" => stdin_bytes.len(),
            },
        );

        // EAC-FR-CPEP: the run is cancelled through a token linked to the
        // caller's, so a durable failure never sets the caller's own flag.
        let run_cancel = request.cancellation.linked();
        let observer = LaunchObserver {
            descriptor,
            carry_len: protocol::longest_secret(
                &secrets.iter().map(String::as_str).collect::<Vec<_>>(),
            )
            .saturating_sub(1),
            secrets,
            sink: request.activity.as_ref(),
            durable: request.durable_output.as_ref(),
            cancel: run_cancel.clone(),
            durable_failure: std::sync::Mutex::new(None),
            log: sink.clone(),
            logs_activity: request.supplementary_mount.is_none(),
            vendor: vendor.clone(),
            container: container_name.clone(),
            carry: std::sync::Mutex::new((Vec::new(), Vec::new())),
        };

        // EAC-FR-34: the invocation and the task, before the run that answers
        // them. What was asked and how it was asked are the first two questions
        // anybody debugging a turn has, and neither is recoverable afterwards —
        // the container is gone and the argv was never written down.
        //
        // The summary is cut from the *masked* text, not from the raw text.
        // A summary is a shortened line rather than a different one, so text
        // that a payload may not carry a summary may not carry either — and a
        // cut applied before the mask only hides a credential when the cut
        // happens to fall in front of it, which is chance rather than a rule
        // (EAC-FR-29).
        let argv_text = observer.mask(&argv.join(" "));
        let (payload, payload_truncated) = bounded_payload(&argv_text);
        observer.emit(AgentActivityEvent {
            at: crate::notes::now_rfc3339(),
            channel: CHANNEL_EXECUTOR,
            kind: descriptor::ActivityKind::Invocation.as_str(),
            summary: descriptor::summary_line(&format!("{} {}", descriptor::DOCKER_PROGRAM, argv_text)),
            payload,
            payload_truncated,
        });
        let task_text = observer.mask(&String::from_utf8_lossy(&stdin_bytes));
        let (payload, payload_truncated) = bounded_payload(&task_text);
        observer.emit(AgentActivityEvent {
            at: crate::notes::now_rfc3339(),
            channel: CHANNEL_EXECUTOR,
            kind: descriptor::ActivityKind::Task.as_str(),
            summary: descriptor::summary_line(&observer.mask(&request.task.instruction)),
            payload,
            payload_truncated,
        });

        let outcome = runtime
            .run(RunRequest {
                argv: &argv,
                container: Some(&container),
                env: &env,
                stdin: &stdin_bytes,
                timeout: Duration::from_millis(request.task.execution.timeout_ms),
                cancel: run_cancel.clone(),
                stdout_limit: protocol::LIMIT_STDOUT,
                stderr_limit: protocol::LIMIT_STDERR,
                observer: Some(&observer),
            })
            .await;

        // EAC-FR-26: removal is confirmed on *every* path out, including the
        // one where the run itself failed. `--rm` handles a clean exit; a
        // killed client does not necessarily take the container with it.
        // The answer is not discarded. A removal that fails means a container
        // is still running under a name that will never be reused, holding the
        // worktree mount and — for Claude Code — a live token in its
        // environment. Nothing here can clean that up on the caller's behalf,
        // but it is exactly the kind of handled failure that is invisible
        // unless it is reported (EAC-FR-29), so it is logged and returned.
        let container_removed = runtime.remove_container(&container_name).await.is_ok();
        if !container_removed {
            logging::log_error(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "agent container could not be removed",
                log_fields! {
                    "vendor" => vendor.as_str(),
                    // The generated name, which is not sensitive and is the one
                    // thing that makes the leftover findable by hand.
                    "container" => container_name.as_str(),
                },
            );
        }

        let outcome = outcome.map_err(|error| match error {
            RuntimeError::Unavailable => AgentExecutionError::RuntimeUnavailable,
            RuntimeError::ImageUnavailable => AgentExecutionError::ImageUnavailable(vendor.clone()),
            RuntimeError::LaunchFailed(reason) => AgentExecutionError::LaunchFailed(reason),
        })?;

        // EAC-FR-ZVRP: a run stopped for a durable failure is reported as that
        // failure, whatever the process outcome was. It is returned after the
        // container removal above, so nothing is left running behind it.
        if let Some(failure) = observer.take_durable_failure() {
            return Err(AgentExecutionError::DurableOutputFailed(failure));
        }

        // --- 8. What the run means --------------------------------------------
        // The identity is asserted back only where the executor supplied it: on
        // a resumed turn the session is the vendor's own, and there is nothing
        // of ours for it to have to match.
        let expected_session_id = (descriptor.assigns_session_id() && resume_session.is_none())
            .then_some(assigned_session_id.as_str());

        let (mut execution, invalid) = self.interpret(
            descriptor,
            outcome,
            started,
            container_removed,
            expected_session_id,
        );

        // EAC-FR-30's last clause. One pinned vendor cannot fail a resume: asked
        // to continue a session it no longer holds, it starts a fresh one and
        // reports success (`CDX-FR-22`). The only thing that distinguishes the
        // two is the identity the run came back with, so that is what decides
        // it rather than the run's own verdict — otherwise a caller believes it
        // continued a conversation that in fact began again with no memory.
        if let (Some(asked), Some(got)) = (
            resume_session,
            execution
                .session
                .as_ref()
                .and_then(|s| s.session_id.as_deref()),
        ) {
            if asked != got {
                logging::log_warn(
                    sink,
                    &BUFFER,
                    &[Domain::Ai, Domain::Backend],
                    "agent session resumption did not continue the requested session",
                    log_fields! { "vendor" => vendor.as_str() },
                );
                execution.resume_unavailable = true;
            }
        }

        logging::log_info(
            sink,
            &BUFFER,
            &[Domain::Ai, Domain::Backend],
            "agent execution finished",
            log_fields! {
                "vendor" => vendor.as_str(),
                "container" => container_name.as_str(),
                "outcome" => execution.process_outcome.as_str(),
                "exit_code" => execution.exit_code,
                "agent_outcome" => execution
                    .response
                    .as_ref()
                    .map(|r| serde_json::to_value(r.outcome).unwrap_or(serde_json::Value::Null)),
                "stdout_truncated" => execution.stdout.truncated,
                "stderr_truncated" => execution.stderr.truncated,
                "duration_ms" => execution.duration_ms,
            },
        );
        // EAC-FR-THUT: the run's own channel says how the run ended, so a
        // watched turn that was stopped does not just end mid-command. The
        // line holds counts and codes alone, so it needs no mask.
        let finished = super::finish::finished_line(
            execution.process_outcome,
            execution.exit_code,
            execution.duration_ms,
            request.task.execution.timeout_ms,
        );
        observer.emit(AgentActivityEvent {
            at: crate::notes::now_rfc3339(),
            channel: CHANNEL_EXECUTOR,
            kind: descriptor::ActivityKind::Finished.as_str(),
            summary: finished.clone(),
            payload: finished,
            payload_truncated: false,
        });
        // EAC-FR-DWGS: the `finished` activity is the last one a durable sink
        // is given, and its failure is the call's failure (EAC-FR-ZVRP).
        if let Some(failure) = observer.take_durable_failure() {
            return Err(AgentExecutionError::DurableOutputFailed(failure));
        }
        if execution.process_outcome != ProcessOutcome::Completed {
            // EAC-FR-29: the excerpt is masked against everything this launch
            // handed the container that a record may not carry — the secrets in
            // its environment, the credential paths that are a credential's
            // location rather than its value, and the session identities. The
            // session ids are here because EAC-FR-29 bars a session or resume
            // field from *any* record without exception, and a CLI that cannot
            // find a session names it in the sentence that says so — which is
            // exactly the sentence this excerpt exists to carry.
            // EAC-FR-42: an execution carrying the semantic-rebase mount gets
            // no excerpt at all. Its streams are file contents rather than
            // diagnostics, and an excerpt cut from them would put specification
            // text in the session buffer by the one route nobody writes on
            // purpose.
            let stderr_excerpt = if observer.logs_activity {
                protocol::stderr_excerpt(
                    &execution.stderr.bytes,
                    execution.stderr.truncated,
                    &observer.secret_refs(),
                )
            } else {
                None
            };
            logging::log_warn(
                sink,
                &BUFFER,
                &[Domain::Ai, Domain::Backend],
                "agent execution did not complete",
                log_fields! {
                    "vendor" => vendor.as_str(),
                    // The one thing that makes a failed turn findable while it
                    // is still on the machine: `docker logs` is gone with the
                    // container, but the name is what a reader searches their
                    // own shell history and this session's INFO records for.
                    "container" => container_name.as_str(),
                    "outcome" => execution.process_outcome.as_str(),
                    // Why the output was rejected, where that is the reason.
                    // "invalid_structured_output" alone leaves a reader unable
                    // to tell prose from a schema violation from a stream that
                    // was cut short.
                    "reason" => invalid.as_ref().map(EnvelopeInvalid::as_str),
                    // EAC-FR-29 keeps every byte of both streams out of a
                    // record. Their shape is not a byte of them, and it is what
                    // separates the failures a reader would otherwise have to
                    // guess between: a CLI that refused its arguments writes a
                    // line to stderr and nothing to stdout, one that died
                    // mid-answer writes to stdout and stops, and one that never
                    // started writes to neither.
                    "exit_code" => execution.exit_code,
                    "stdout_bytes" => execution.stdout.bytes.len(),
                    "stderr_bytes" => execution.stderr.bytes.len(),
                    "stdout_truncated" => execution.stdout.truncated,
                    "stderr_truncated" => execution.stderr.truncated,
                    // The same number the INFO record above carries, not a
                    // second reading of the clock. Two records a reader lines up
                    // side by side should not disagree about how long one run
                    // took.
                    "duration_ms" => execution.duration_ms,
                    // The sentence the CLI itself wrote about why it stopped.
                    // Everything above narrows a failure down; this is what
                    // names it, and without it a reader has to reproduce the
                    // container by hand to read one line.
                    "stderr_excerpt" => stderr_excerpt,
                },
            );
        }

        execution.duration_ms = started.elapsed().as_millis() as u64;
        Ok(execution)
    }

    /// EAC-FR-06. Routed through `FsAccess` rather than a bare path predicate,
    /// so a directory outside the session's allowlist is refused here rather
    /// than mounted into a container.
    fn validate_directory(
        &self,
        fs: &FsAccess,
        path: &std::path::Path,
    ) -> Result<PathBuf, AgentExecutionError> {
        use AgentExecutionError::ExecutionDirectoryInvalid as Invalid;

        let info = fs
            .file_info(path)
            .map_err(|error| Invalid(directory_problem(&error)))?;
        if info.kind != EntryKind::Dir {
            return Err(Invalid(DirectoryProblem::NotADirectory));
        }
        // EAC-FR-02 states the execution directory is a **canonical** one, and
        // EAC-FR-ZKMR is the first rule to depend on it: the container is given
        // this path, and Git resolves the repository through paths this module
        // reads back from Git in canonical form. Two spellings of one directory
        // would mount a store the worktree's own records do not name.
        Ok(crate::changes::canonicalize_lenient(path))
    }

    /// Turn a runtime outcome into the two-axis result (EAC-FR-19, EAC-FR-22,
    /// EAC-FR-23).
    fn interpret(
        &self,
        descriptor: &descriptor::VendorExecutionDescriptor,
        outcome: runtime::RuntimeOutcome,
        started: Instant,
        container_removed: bool,
        expected_session_id: Option<&str>,
    ) -> (AgentExecution, Option<EnvelopeInvalid>) {
        let base = |process_outcome: ProcessOutcome, response, session| AgentExecution {
            process_outcome,
            exit_code: outcome.exit_code,
            response,
            stdout: outcome.stdout.clone(),
            stderr: outcome.stderr.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
            resume_unavailable: false,
            session,
            container_removed,
        };

        match outcome.end {
            RunEnd::TimedOut => return (base(ProcessOutcome::Timeout, None, None), None),
            RunEnd::Cancelled => return (base(ProcessOutcome::Cancelled, None, None), None),
            RunEnd::Terminated => return (base(ProcessOutcome::Terminated, None, None), None),
            RunEnd::Exited => {}
        }

        // A non-zero exit is not an occasion to look for an envelope: the agent
        // did not finish its turn, so whatever is on stdout is a fragment of
        // one at best.
        if outcome.exit_code != Some(0) {
            return (base(ProcessOutcome::NonZeroExit, None, None), None);
        }

        // EAC-FR-23: a truncated stdout is a prefix, and a prefix is never
        // parsed. Parsing one would risk accepting a document that happens to
        // close early as a complete report.
        if outcome.stdout.truncated {
            return (
                base(ProcessOutcome::InvalidStructuredOutput, None, None),
                Some(EnvelopeInvalid::OutputTruncated),
            );
        }

        let Some(text) = outcome.stdout.as_utf8() else {
            return (
                base(ProcessOutcome::InvalidStructuredOutput, None, None),
                Some(EnvelopeInvalid::NotUtf8),
            );
        };

        match descriptor.extract(text, expected_session_id) {
            Ok((envelope, vendor_session)) => {
                // The vendor's own session id fills in where the agent left one
                // out: an agent does not reliably know the id its CLI assigned,
                // and without it EAC-FR-30's resume has nothing to carry.
                let session = merge_session(envelope.session.clone(), vendor_session);
                (base(ProcessOutcome::Completed, Some(envelope), session), None)
            }
            Err(reason) => (
                base(ProcessOutcome::InvalidStructuredOutput, None, None),
                Some(reason),
            ),
        }
    }
}
