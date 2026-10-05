# synthesis-server

The Synthesis backend microservice. One standalone Rust service, built on Axum
over Tokio, packaged as a distroless Linux container image. It answers the
unauthenticated endpoint `GET /v1/health`, which reports the version the binary
was built from and the capabilities it serves, and the authenticated REST
surface and WebSocket endpoints of the application service and the relay.

The crate holds two responsibilities. The **application service** owns identity,
teams, organizations, memberships, invitations, devices, projects, project
access control, drafts, and conversations. The **relay boundary** owns the
routing state between IDE workers and remote clients, and reads no application
payload. V1 holds every record in memory, and it carries the **remote session**
of one IDE and its mobile clients over three WebSocket endpoints.

Specifications:
[`specifications/server/BMS-backend-microservice.md`](../specifications/server/BMS-backend-microservice.md),
[`specifications/server/SAS-server-application-service.md`](../specifications/server/SAS-server-application-service.md),
[`specifications/server/SRB-server-relay-boundary.md`](../specifications/server/SRB-server-relay-boundary.md),
[`specifications/server/RSN-remote-session.md`](../specifications/server/RSN-remote-session.md),
[`specifications/server/WSK-websocket.md`](../specifications/server/WSK-websocket.md).

The whole REST surface is described by [`api/openapi.yaml`](../api/openapi.yaml).

## Build, test, and run

The crate is its own workspace root, so every command runs from `server/`. The
`--locked` flag makes the committed `Cargo.lock` the sole source of dependency
versions: a lock file that disagrees with the manifest fails the command rather
than being regenerated.

```bash
cd server
cargo build --locked          # binary at target/debug/synthesis-server
cargo test  --locked
cargo run   --locked
```

The Rust toolchain is the one `src-tauri/rust-toolchain.toml` pins, which is the
repository's single pin. To apply it here locally, copy it in — the copy is
ignored by `server/.gitignore`, so it is never committed as a second pin:

```bash
cp ../src-tauri/rust-toolchain.toml ./rust-toolchain.toml
```

## Configuration

```text
synthesis-server [--host <ip-address>] [--port <port>]
```

| Setting | Option | Environment variable | Default |
| --- | --- | --- | --- |
| Bind address | `--host` | `SYNTHESIS_SERVER_HOST` | `0.0.0.0` |
| Port | `--port` | `SYNTHESIS_SERVER_PORT` | `8080` |

The option has precedence over the variable, and the variable has precedence
over the default. An environment variable that holds an empty string counts as
unset.

The default port is above 1024, so the container's non-root user binds it
without an added capability.

Two more settings are read from the environment alone, and both are needed:

| Setting | Environment variable | Accepted form |
| --- | --- | --- |
| Administrator user identifier | `SYNTHESIS_SERVER_ADMIN_ID` | a UUID |
| Static bearer token | `SYNTHESIS_SERVER_TOKEN` | at least 16 characters |

No command-line option carries either of them: a command line is readable by
every other process on the host, and one of the two values is a credential. A
value that is missing, empty, or malformed is a startup failure with status
**2**, and the diagnostic names the variable rather than the value. The token
reaches no log record, no answer, no error message, and no persisted record.

At startup the service creates the administrator user with the configured
identifier, or reconciles the existing record to the administrator role.
Repeated starts with one identifier create one user.

A value that is not a valid IP address, or not a valid port number in
`1..=65535`, is a startup failure: the process writes one diagnostic to standard
error and exits with status **2**, having bound no socket. An option with no
value after it, and an argument the service does not know, are refused on the
same terms and with the same status.

A failure to bind the resolved address and port exits with status **1**, so an
operator tells a bind failure from a setting that was refused.

After the listener is bound and before the first request is served, the process
writes one record to standard output:

```json
{"event":"listening","address":"0.0.0.0","port":8080,"version":"v2.0.0"}
```

## Records

Every record is one line of JSON on standard output. Beside the records of the
listener and of the shutdown, the service writes one record for each served
application request and one for each relay operation:

```json
{"event":"request","method":"POST","route":"/v1/projects/{project_id}/drafts","status":201,"records":{"project_id":"…","id":"…"}}
{"event":"relay","operation":"register_worker","connection_id":"…","instance_id":"…","outcome":"ok"}
```

A request record names the route template, never the path, so an identifier
reaches it as a named parameter alone. A relay record names the connection
identifier, the `instance_id`, and the operation.

No record holds the token, a client handle, a key, an email address, Draft
content, or a Conversation message body. The health route writes no record.

## Shutdown

The process shuts down gracefully on `SIGTERM` and on `SIGINT`. It stops
accepting new connections, lets the requests already in flight complete, and
exits with status 0. A request that is still in flight after a drain period of
10 seconds is dropped, and the process still exits 0. A second signal during the
drain exits immediately.

## The HTTP surface

```text
GET  /v1/health -> 200 OK
                   content-type: application/json
                   {"version":"<resolved build version>",
                    "capabilities":["remote_session","websocket"]}

HEAD /v1/health -> 200 OK
                   content-type: application/json
                   no body
```

The `GET` body is one JSON object with exactly two members: `version`, a string,
and `capabilities`, the stable identifiers of the capabilities this relay
serves. V1 advertises exactly `remote_session` and `websocket`, each naming one
specification; a client treats an identifier it does not know as unsupported.
The answer holds no key, no QR content, no device registration, no application
payload, no user, no project ownership, and no connection state.

A `HEAD` request asks for the `GET` answer without its body, so it returns the
same status and the same headers and no body. A container orchestrator can
therefore probe the service with either method.

`/v1/health` is the one route that needs no credential: any other method on it
is answered `405 Method Not Allowed`, and a path that no route of the service
declares is answered `404 Not Found`.

The path is matched exactly: `/V1/Health` and `/v1/health/` are both answered
`404`. A query string names no route and changes nothing.

The handler answers from the compiled-in version constant and the compiled-in
capability list alone. It reads no file, opens no connection, and consults no
clock, so every call in the life of a process returns the same bytes.

### The authenticated surface

Every other route, and every WebSocket upgrade, needs the header
`Authorization: Bearer <token>`, carrying the configured `SYNTHESIS_SERVER_TOKEN`.
The token authenticates transport access alone: it names no client, and the
handle check, the worker-ownership check, and the end-to-end client
authentication of the remote session stand beside it rather than behind it. The service compares the presented value in constant
time, and a request that carries no header, another scheme, or another value is
answered `401 Unauthorized` with the body `{"error":{"code":"unauthenticated","message":"…"}}`.

In V1 the token authenticates the persisted administrator user, which bypasses
every project access-control check. The ownership fields and the grants are
stored on the records all the same, so a later authentication mode — an OIDC
provider that maps `(issuer, subject)` onto a persisted user — changes neither
the ownership model nor the access-control model.

The routes are grouped as follows, and `api/openapi.yaml` describes each one
with its request, its answer, and the permission it needs:

```text
/v1/users            /v1/teams          /v1/organizations
/v1/memberships      /v1/invitations    /v1/devices
/v1/roles            /v1/projects       /v1/projects/{project_id}/grants
/v1/projects/{project_id}/drafts        /v1/drafts/{draft_id}
/v1/projects/{project_id}/conversations /v1/conversations/{conversation_id}
/v1/relay/workers    /v1/relay/workers/{instance_id}
/v1/relay/projects
```

### The WebSocket endpoints

```text
GET /v1/worker        upgrade -> the worker protocol
GET /v1/client        upgrade -> the client protocol
GET /v1/registration  upgrade -> the registration protocol
```

Each upgrade carries the same bearer token. A request to one of the three paths
that is not an upgrade is answered `400 Bad Request`.

Every message is exactly one UTF-8 JSON object carrying `protocol`, `version`,
`type`, an optional `request_id`, and `body`. The `protocol` must equal the
protocol of the endpoint, so a frame of one protocol is never accepted as a
frame of another. A frame larger than 1 MiB is refused. Binary values, keys,
proofs, and encrypted payloads are unpadded base64url strings, and the relay
reads none of them.

- **Worker.** The first frame is `worker_hello`, which names the `instance_id`,
  the complete exposed-project list, and the selected project. The relay answers
  `worker_ready`. The worker then sends `projects_changed`,
  `handle_invalidated`, `registration_result`, `client_authentication_result`,
  `application_message`, and `delivery_failed`, and receives `registration_start`,
  `registration_payload`, `client_authenticate`, and `application_message`.
- **Client.** The first frame is `client_attach`, carrying the opaque handle and
  the target `instance_id`; the relay answers `client_attached`. Before the IDE
  accepts the connection, only `client_attach`, `client_authenticate`,
  `client_authentication_result`, `protocol_reset`, and transport-error frames
  are routed. After acceptance the connection carries `application_message` in
  both directions, and receives `worker_replaced` when a replacement worker
  takes the route.
- **Registration.** The first frame is `registration_start`; the relay assigns a
  fresh opaque handle and answers `registration_bound`. `registration_payload`
  frames pass to the worker unchanged, and the IDE sends exactly one terminal
  `registration_result`.

A relay-generated failure is a `relay_failure` frame on the worker and client
endpoints, and a `registration_failed` frame on the registration endpoint. Its
reason, and the reason a close frame carries, come from one vocabulary:
`unsupported_version`, `protocol_mismatch`, `invalid_frame_shape`,
`missing_worker_authentication`, `invalid_worker_authentication`,
`worker_not_found`, `registration_not_found`, `handle_not_found`,
`handle_expired`, `handle_revoked`, `handle_reset`, `worker_replaced`,
`delivery_failed`, `relay_restarted`, and `protocol_reset`. The relay closes an
unrecoverable protocol error with the WebSocket close code `1008` and the reason
as the close reason, and reports delivery success for no frame it did not
deliver.

`GET /v1/relay/projects` reports the projects one worker exposes to one
registered client. Beside the bearer token the request carries
`X-Synthesis-Client-Handle` and `X-Synthesis-Instance-Id`, and the answer holds
`instance_id`, `selected_project_id`, and `projects` — each project an opaque
`project_id` and a `display_name` — and nothing else. A worker announces its
projects over its own WebSocket alone; the relay serves no route through which a
worker publishes them.

Every error answer is one JSON object with an `error` member holding a `code`
and a `message`. The codes are `unauthenticated`, `forbidden`, `not_found`,
`invalid_field`, `invalid_role`, `invalid_permission`, `duplicate_id`,
`duplicate_membership`, `duplicate_grant`, `owner_protected`, `referenced`,
`stale_revision`, `conversation_locked`, `invitation_not_pending`, and
`internal`.

### The project roles

| Role | Permissions |
| --- | --- |
| `viewer` | `project.read`, `draft.read`, `conversation.read` |
| `contributor` | the viewer permissions, `draft.create`, `draft.update`, `draft.archive`, `conversation.create`, `conversation.append`, `conversation.update`, `conversation.resolve` |
| `administrator` | the contributor permissions, `project.update`, `project.manage_acl`, `draft.delete`, `draft.graduate`, `conversation.lock` |
| `owner` | every permission |

`GET /v1/roles` publishes the same mapping, so a client reads it rather than
assuming it. The effective permissions of a user are the union of the grants to
that user, of the grants to the teams of which the user holds an active
membership, and of the grants to the organizations of which the user holds an
active membership. Grants are additive, there is no deny grant, and the owner
grant is changed by a transfer of ownership alone.

### Persistence and restart

V1 holds every record in memory behind the repository ports. A restart loses
every user, team, organization, membership, invitation, device, project, grant,
draft, and conversation, then reconciles the administrator user again. The relay
loses every worker route and every client handle as well: the relay reports no
session as active, a worker reconnects and announces again, and a client whose
handle the restart dropped holds a new registration before it is routed again. A
graceful shutdown closes each open WebSocket with `relay_restarted`, so a client
distinguishes a stopping relay from a protocol failure of its own. A durable adapter replaces the in-memory one
behind the same ports, and changes no part of the REST contract.

## The build version

The version is resolved when the crate is compiled, by `build.rs`, from the
first of these sources that yields a value:

1. the `SYNTHESIS_BUILD_VERSION` build argument, which carries the Git tag of
   the GitHub Release the image is built from;
2. the exact Git tag at the commit being built, when that commit carries one;
3. the short Git commit hash, when Git metadata is readable but no tag points at
   the commit;
4. the literal string `undefined`, when no source above yields a value.

The resolved value is compiled into the binary. The running process never
invokes `git`, never reads a `.git` directory, and never reads an environment
variable to learn its version, so the image — which holds neither a Git
executable nor a checkout — reports the value that was chosen when it was built.

A Git command that fails, or that is absent, is not a build failure: the build
script falls to the next source in the order.

## The image

| Property | Value |
| --- | --- |
| Image name | `ghcr.io/<owner>/<repository>` (the GitHub repository name, in lower case) |
| Tag | the GitHub Release tag, and no other tag |
| Platforms | `linux/amd64`, `linux/arm64` |
| Entrypoint | `/usr/local/bin/synthesis-server` |
| Port | `8080` |
| User | the base image's non-root user, UID `65532` |
| Build argument | `SYNTHESIS_BUILD_VERSION` — the release tag, read by the build script |

The final stage is a distroless base, pinned by an immutable digest rather than
by a tag. It holds the service binary and the CA certificate bundle its base
provides, and nothing else: no shell, no package manager, no compiler, no
interpreter, and no source file. It declares no volume and holds no credential
of any kind.

The entrypoint is the binary itself, so the arguments a container is started
with are the binary's own arguments and nothing wraps or rewrites them.

Build and run the image locally, from `server/`:

```bash
# One platform, for the machine you are on.
docker build -t synthesis-server:local .
docker build --build-arg SYNTHESIS_BUILD_VERSION=v2.0.0 -t synthesis-server:v2.0.0 .

# Both supported platforms, as one manifest list.
docker buildx build --platform linux/amd64,linux/arm64 \
  --build-arg SYNTHESIS_BUILD_VERSION=v2.0.0 \
  -t synthesis-server:v2.0.0 .

# Run it and read the health endpoint.
docker run --rm -p 8080:8080 synthesis-server:local
curl -s http://127.0.0.1:8080/v1/health
# {"version":"undefined"}

# Override the bind address and the port.
docker run --rm -p 9099:9099 \
  -e SYNTHESIS_SERVER_PORT=9099 synthesis-server:local
docker run --rm -p 9099:9099 synthesis-server:local --port 9099
```

The image name `synthesis-server` is this crate's package name and carries no
registry host, so a local image is never mistaken for the published one and
cannot be pushed without being tagged again.

### Every image at once — `task docker`

`task docker` builds every image the repository defines, including this one,
from the repository root. It is documented in full in the
[repository README](../README.md) and specified by
[`specifications/infra/TSK-taskfile.md`](../specifications/infra/TSK-taskfile.md).

```bash
task docker
```

It resolves one local build version for the whole invocation — the exact Git tag
at `HEAD`, otherwise the short Git commit hash, otherwise the literal string
`undefined` — and applies it to this image as:

- the local image name and tag `synthesis-server:<resolved version>`, and
- the build argument `SYNTHESIS_BUILD_VERSION=<resolved version>`,

so a container started from the image reports that version from
`GET /v1/health`. The tag is never `latest`.

**It publishes nothing.** It pushes no image, logs in to no registry, reads no
credential, and changes no release metadata. The image exists in the local image
store of the machine that ran it and nowhere else.

## The pipelines

Both workflows are defined by
[`specifications/infra/CIP-ci-pipeline.md`](../specifications/infra/CIP-ci-pipeline.md),
which is the sole authority on their triggers, permissions, and job structure.
This section states that behaviour rather than defining it.

The division is deliberate: `CI / server` validates the crate on every pull
request, while the image is built and published **only** when a GitHub Release
is published.

### Verification — `.github/workflows/ci.yml`, named `CI`

| Property | Value |
| --- | --- |
| Trigger | `pull_request` (`opened`, `synchronize`, `reopened`), with no path filter and no `push` trigger |
| Job | `server`, reported as the check `CI / server` |
| Commands | `cargo build --locked` and `cargo test --locked`, from `server/` |
| Toolchain | copied in from `src-tauri/rust-toolchain.toml`, the repository's single pin |
| Permissions | `contents: read` at the workflow level, and nothing else. No job holds `packages: write` |
| Credentials | none. The workflow reads no repository secret and logs in to no registry |

`CI / server` blocks a merge through the aggregate check `CI / gate`, and
through no check of its own. **`CI / gate` is the only status check branch
protection requires**; `CI / server` is listed in that job's `needs`. A pull
request that breaks a test in this crate therefore fails `CI / server` and fails
`CI / gate`, and that run builds and publishes no image.

The verification workflow builds no container image, pushes none, and holds no
registry credential in any job.

### Publication — `.github/workflows/server-image.yml`, named `Server image`

| Property | Value |
| --- | --- |
| Trigger | the `release` event with `types: [published]`, and no other trigger |
| Job | `publish` |
| Permissions | `contents: read` at the workflow level; the `publish` job alone adds `packages: write` |
| Credentials | the run's `GITHUB_TOKEN` and nothing else — no personal access token and no registry password |
| Output | one manifest list at `ghcr.io/<owner>/<repository>:<release tag>`, holding `linux/amd64` and `linux/arm64` |

The job passes the release tag into the image build as
`SYNTHESIS_BUILD_VERSION`, so the published image reports that tag from
`GET /v1/health`. The release tag is the only tag pushed: no `latest`, no
commit-hash tag, and no branch tag.

Because the trigger is the publication of a release rather than the creation of
a tag, the workflow runs for a stable release and for a pre-release alike, and
does not run for a draft. A draft that is published later is built at that
moment.

**No check of the publishing workflow is required by any branch-protection
rule.** A failed publication never blocks a merge, and a merge never waits on a
publication. A failure of any step of `publish` — the login, the build for
either platform, or the push — fails the job and the run, so a release whose
image did not publish is visible as a failed run rather than as a silent
absence.
