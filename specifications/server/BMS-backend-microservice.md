# Backend microservice

**Spec code:** `BMS`

## Intent
The server-side foundation Synthesis IDE reaches when a capability cannot live in the desktop process alone. It is one standalone Rust service, built on Axum over Tokio, packaged as a distroless Linux container image, and it answers the unauthenticated endpoint `GET /v1/health`, which reports the version the binary was built from and the capabilities it serves. It exists so that the parts of a client-server capability that are tedious and easy to get wrong — the crate layout, the configuration surface, the bind and shutdown behaviour, the image, and the pipelines that validate and publish it — are decided once and stay correct as the service gains its application surface. This specification owns the foundation alone. The application routes, the identity and access-control model, and the persistence ports are `SAS-server-application-service.md`'s; the relay boundary and the relay routing state are `SRB-server-relay-boundary.md`'s. The WebSocket endpoints and the frame contracts are `WSK-websocket.md`'s, and the remote-session rules are `RSN-remote-session.md`'s. Out of scope: the frame and session protocols themselves, service-to-service protocols, and TLS termination — each belongs to a specification that defines its own contract, and none of them is anticipated here by a demonstration endpoint or a placeholder route.

## Contract surface
This specification owns the crate, the executable and its bind configuration, the health route, the build-time version, the container image, and the documentation. The application routes and their configuration belong to `SAS-server-application-service.md`, and the relay routes belong to `SRB-server-relay-boundary.md`. It exposes no Tauri command, is not reachable from `src/**` or `src-tauri/**`, and shares no code with them.

### The crate and the executable
```text
server/Cargo.toml          package `synthesis-server`
server/Cargo.lock          committed; this crate is its own workspace root
server/src/lib.rs          the library root, which holds the layers
server/src/main.rs         the binary entry point
server/Dockerfile          the image definition
server/README.md           the documentation
server/.gitignore          /target/ and the runner-local Rust pin copy
```

- **Build** — `cargo build --locked`, from `server/`.
- **Test** — `cargo test --locked`, from `server/`.
- **Run** — `cargo run --locked`, from `server/`.
- **Binary** — `server/target/debug/synthesis-server` (`target/release/…` for a release build), plus the platform's executable suffix.

### The command line and the environment
```text
synthesis-server [--host <ip-address>] [--port <port>]
```

| Setting | Option | Environment variable | Default |
| --- | --- | --- | --- |
| Bind address | `--host` | `SYNTHESIS_SERVER_HOST` | `0.0.0.0` |
| Port | `--port` | `SYNTHESIS_SERVER_PORT` | `8080` |

The option has precedence over the variable, and the variable has precedence over the default.

### The HTTP surface
```text
GET  /v1/health -> 200 OK
                   content-type: application/json
                   {"version":"<resolved build version>",
                    "capabilities":["remote_session","websocket"]}

HEAD /v1/health -> 200 OK
                   content-type: application/json
                   no body
```

This is the whole of the unauthenticated surface. No method other than `GET` and `HEAD` is accepted on this path. Every other route and every WebSocket upgrade the service serves is authenticated and is defined by `SAS-server-application-service.md`, `SRB-server-relay-boundary.md`, `RSN-remote-session.md`, and `WSK-websocket.md`.

### The image
```text
Image name  ghcr.io/<owner>/<repository>          (the GitHub repository name, in lower case)
Tag         the GitHub Release tag, and no other tag
Platforms   linux/amd64, linux/arm64
Entrypoint  /usr/local/bin/synthesis-server
Port        8080
User        the base image's non-root user, UID 65532
Build arg   SYNTHESIS_BUILD_VERSION — the release tag, read by the build script
```

### The pipelines
`../infra/CIP-ci-pipeline.md` is the sole authority on both workflows and their check names. The crate is validated on every pull request by the `CI / server` lane (CIP-FR-21), which blocks a merge through `CI / gate`. The image is built and published only when a GitHub Release is published, by the separate workflow of CIP-FR-24. Nothing here restates their triggers, permissions, or job structure.

## Functional requirements
1. **BMS-FR-01** The service is a standalone crate at `server/`, with `server/Cargo.toml` declaring the package `synthesis-server` and with its own committed `Cargo.lock`. The repository root holds no Cargo manifest, so the crate is its own workspace root.
2. **BMS-FR-02** The crate depends on neither `synthesis_lib`, nor Tauri, nor any other code under `src-tauri/`. It builds and tests in a checkout that holds no `src-tauri/` directory at all, and it links against no system library that the application's Rust lane installs packages for (per `../infra/CIP-ci-pipeline.md` CIP-FR-09).
3. **BMS-FR-03** The crate builds one library target and one binary target, `synthesis-server`. The binary holds the process entry point alone, the library holds every layer, and `main` starts the HTTP service and returns only after the service has stopped.
4. **BMS-FR-04** The HTTP service is an Axum router served on a Tokio runtime. The router is built by one constructor function that takes the service state and returns the router, so a test drives the same router the binary serves without opening a socket, and another specification adds its routes by merging its own router into that one function.
5. **BMS-FR-05** The bind address and the port are each configurable through a command-line option and through an environment variable, as the contract surface names them. The option wins over the variable, the variable wins over the default, and the defaults are `0.0.0.0` and `8080`.
6. **BMS-FR-06** The default port is above 1024, so the process binds it as the non-root user of BMS-FR-17 without an added capability.
7. **BMS-FR-07** A value that is not a valid IP address, or not a valid port number in 1..=65535, is a startup failure. The process writes one diagnostic to stderr that names the setting, the value it refused, and the accepted form; it then exits with status 2. It binds no socket and serves no request first.
8. **BMS-FR-08** A failure to bind the resolved address and port — the address is in use, or the process may not have it — is reported as a diagnostic on stderr that names the address and the port, and the process exits with status 1. The status distinguishes a bind failure from the configuration failure of BMS-FR-07.
9. **BMS-FR-09** After the listener is bound and before the first request is served, the process writes one structured record to stdout that names the bound address, the bound port, and the resolved build version, so an operator can read from the log alone which build is running and where it is listening.
10. **BMS-FR-10** The process shuts down gracefully on the termination signals of the target platform — `SIGTERM` and `SIGINT` on Linux. It stops accepting new connections, lets the requests already in flight complete, and exits with status 0. Requests still in flight after a drain period of 10 seconds are dropped and the process still exits 0, so a stuck connection cannot keep a container alive until its runtime kills it. A second signal during the drain exits immediately.
11. **BMS-FR-11** `GET /v1/health` returns HTTP `200 OK`, the response header `content-type: application/json`, and a body that is one JSON object with exactly two members: `version`, a string, and `capabilities`, an array of stable capability identifiers. The object holds no other member.
12. **BMS-FR-12** The health handler answers from the compiled-in version constant and the compiled-in capability list alone. It reads no file, opens no network connection, consults no clock, and holds no state, so every call in the life of a process returns the same bytes.
    - *Why:* An answer that reported live state would disclose whether anyone is connected, which the unauthenticated route must never do.
13. **BMS-FR-JQZW** The capability list of BMS-FR-11 is exactly `remote_session` and `websocket`, in that order. Each identifier names one dedicated specification, and the response holds no key, no QR content, no device registration, no application payload, no user, no project ownership, and no connection state.
14. **BMS-FR-13** The router carries the route of BMS-FR-11, the routes of `SAS-server-application-service.md`, `SRB-server-relay-boundary.md`, and `RSN-remote-session.md`, and the WebSocket upgrades of `WSK-websocket.md`. A request to a path none of them routes is answered `404 Not Found`. The health route accepts `GET` and `HEAD`: a `HEAD` request asks for the `GET` answer without its body, so it is answered with the same status and the same headers as BMS-FR-11 gives and with no body. A request to `/v1/health` with any other method is answered `405 Method Not Allowed`. The health route is the one route that needs no credential, and neither the `404` nor the `405` carries application content.
    - *Why:* Accepting both methods on the health route is what lets a container orchestrator probe the service with either one.
15. **BMS-FR-14** The build version is resolved when the crate is compiled, by the crate's build script, from the first of these sources that yields a value: the `SYNTHESIS_BUILD_VERSION` build argument, which carries the Git tag of the GitHub Release the image is built from; the exact Git tag at the commit being built, when that commit carries one; the short Git commit hash, when Git metadata is readable but no tag points at the commit; and the literal string `undefined`, when no source above yields a value.
16. **BMS-FR-15** The resolved value is compiled into the binary. The running process never invokes `git`, never reads a `.git` directory, and never reads an environment variable to learn its version, so the image — which holds neither a Git executable nor a checkout — reports the value that was chosen when it was built.
17. **BMS-FR-16** A Git command that the build script runs and that fails, or that is absent, is not a build failure: the build script falls to the next source in the order of BMS-FR-14. The build script declares its rerun conditions, so a changed build argument or a changed Git head produces a rebuild rather than a stale version.
18. **BMS-FR-17** `server/Dockerfile` builds the image in stages. The final stage is a distroless Linux base that is pinned by an immutable digest rather than by a tag, and it runs as that base's non-root user, UID 65532. The compiler, the toolchain, the source tree, and the build cache belong to an earlier stage and reach no layer of the final image.
19. **BMS-FR-18** The final image holds the service binary, the CA certificate bundle its base provides, and nothing else that the service does not need at run time. It holds no shell, no package manager, no compiler, no interpreter, and no source file.
20. **BMS-FR-19** The image's entrypoint is the service binary itself, so the arguments a container is started with are the binary's own arguments and nothing wraps or rewrites them. The image declares the default port of BMS-FR-05 and declares no volume.
21. **BMS-FR-20** No layer, environment variable, or file of the image holds a credential of any kind — no token, no key, no registry login, and no certificate private key. Nothing in the build writes one in, and the image needs none to start.
22. **BMS-FR-21** The image is built for `linux/amd64` and `linux/arm64` from the one Dockerfile, and the two are published together as one manifest list under one tag, so a `docker pull` on either architecture resolves an image that runs there. How each architecture is compiled — through emulation or through cross-compilation — is the build's own choice and is not contractual.
23. **BMS-FR-22** `server/README.md` documents the image entrypoint, the default port, the user the container runs as, the supported target platforms, and the commands that build and run the image locally. A local build uses the image name `synthesis-server`, which is this crate's package name and which carries no registry host, so a local image is never mistaken for the published one of BMS-FR-21 and cannot be pushed without being tagged again. The README documents that name, the direct `docker build` command that applies it, and the `task docker` build of every image the repository defines (per `../infra/TSK-taskfile.md` TSK-FR-13), which tags the server image with the version it resolved, passes that version as `SYNTHESIS_BUILD_VERSION`, and publishes nothing. It also documents the configuration options and variables with their defaults, the health response with its capability list, the bearer-token authentication every other route and upgrade needs, and the version-resolution order of BMS-FR-14.
24. **BMS-FR-23** `server/README.md` also documents the two pipelines: their triggers, their job names, the status check that branch protection requires, the token permissions each holds, and the division that `CI / server` validates the crate on every pull request while the image is built and published only when a GitHub Release is published. The workflow behaviour itself is `../infra/CIP-ci-pipeline.md`'s (CIP-FR-21, CIP-FR-24), and the README states it rather than defining it.
25. **BMS-FR-24** The crate follows the repository's Rust conventions: it is formatted with `cargo fmt` defaults, its modules carry doc comments that name this specification and the requirements they satisfy, its `Cargo.lock` is committed and is the sole source of dependency versions, and its commands are run with `--locked` so a lock file that disagrees with the manifest fails rather than being regenerated.
26. **BMS-FR-25** The crate carries automated tests for the health endpoint that assert the `200 OK` status, the `application/json` content type, and the exact response shape of BMS-FR-11 — the members `version` and `capabilities` and no other — together with the exact capability identifiers of BMS-FR-JQZW. The tests drive the router of BMS-FR-04 and need no published image and no network.
27. **BMS-FR-26** Version resolution is a pure function over its inputs — the build argument, the exact tag, and the short hash, each present or absent — and is tested for each of the four outcomes of BMS-FR-14, including the `undefined` fallback. No test needs a Git checkout, a Git executable, or a tagged commit.
28. **BMS-FR-27** The foundation writes and reads no file at run time, opens no connection to another service, and holds no application logic and no WebSocket handler of its own. Every capability the service serves beyond the health endpoint is defined first by `SAS-server-application-service.md`, `SRB-server-relay-boundary.md`, `RSN-remote-session.md`, or `WSK-websocket.md`.

## Non-functional requirements
- The foundation itself holds no state: the health answer reads the compiled-in version alone. The records and the routes the service holds between requests are the in-memory state of `SAS-server-application-service.md` SAS-FR-PDNU and `SRB-server-relay-boundary.md` SRB-FR-ZUPL, and a restart loses them.
- The service speaks plain HTTP. Whatever terminates TLS is outside the image and outside this specification, which is why the image needs no certificate and no private key.
- Startup is fast enough for a container orchestrator's default probe interval: the process binds its listener without reading a file, resolving a name, or contacting anything.
- The image is as small as the binary, the CA bundle, and the pinned base allow. Nothing is added for convenience, because there is no shell in which convenience could be used.
- A rebuild of an unchanged Dockerfile from the unchanged pinned base produces an image whose behaviour is the same; the build version is the one input that a rebuild at a different commit or under a different build argument changes.
- The crate's dependency floor is Axum, Tokio, the identifier and time crates the application layer needs, and what they need in turn. Every dependency is resolved from the committed `Cargo.lock`.
- Log records carry no credential, no token, and no request body, per `SAS-server-application-service.md` SAS-FR-PMRB.
- The minimum Rust version the crate compiles under is the one `src-tauri/rust-toolchain.toml` pins, which is the repository's single pin and which the `CI / server` lane resolves (per `../infra/CIP-ci-pipeline.md` CIP-FR-10). The image build's compiler is the pinned base of its builder stage and satisfies that floor.
