# Synthesis

Synthesis is a desktop IDE for engineers who build software with AI agents.

You describe a change in plain words and talk it through with AI agents until it is clear. Then you start its implementation. A coding agent, such as Claude Code or Codex, does the work in a Docker container on a Git branch, and a second agent session reviews it. You read the result, merge it, and open a pull request.

Synthesis does not make decisions for you, and it does not replace your coding agent or your skills. It gives you one place to plan the work, to watch it, and to review it. You bring your own AI keys and subscriptions.

> Synthesis is in early development (version 0.1.0). It is for one person on one computer, and you build it from source.

## How you use it

### 1. Write a change

Open the **Changes** panel and click **+ Change**. A change is one Markdown prompt that says what you want: a feature, a fix, or a new requirement. Synthesis keeps it in your repository and commits it to Git, so it stays with the project.

### 2. Discuss it with agents

Mention an agent in a comment, for example `@critic`, or write `@all` to ask every agent of the project. An agent answers in the discussion. It can ask you questions, and it can propose **edits** to the prompt. You accept or reject each edit. The prompt changes only when you accept.

An agent in a discussion cannot read or run your code. It sees only the project material that Synthesis gives it.

### 3. Implement it

When the change is ready, click **Implement**. Synthesis locks the change and starts an **implementation run**:

1. A coding agent does the work in a Docker container.
2. A new agent session reviews the work and answers `ready` or `revise`.
3. On `revise`, the run does another pass. On `ready`, Synthesis commits the result.

You follow each run in the **Implementation** tab of the **Runs** panel. If the agent needs an answer from you, the run waits, and you answer it there.

A run works in a **work stream**: a branch and a working copy that Synthesis owns. Several runs can share one stream, one after the other.

### 4. Review and merge

Open the **Commits** panel to see what a stream changed, file by file, and to commit your own edits. When the work is complete, merge the stream into its base branch. If Git finds a conflict, an agent resolves it, and a second agent reviews the result. Then open a pull request from the **Git** panel.

### Work with GitHub and OpenSpec

- **GitHub.** Publish a change as a GitHub issue, so that an agent outside Synthesis can do it. Or claim a "Ready" task of a GitHub Project: Synthesis makes a change from the issue, and you implement it as usual.
- **OpenSpec.** If your project uses [OpenSpec](https://github.com/Fission-AI/OpenSpec), Synthesis can work in OpenSpec mode. When you implement a change, you can first propose an **OpenSpec change** and then apply it.

## The window

| Panel | What it is for |
| --- | --- |
| Project | The files and artifacts of the project. |
| Documents | Reference documents that agents can search. |
| Notes | Short notes to yourself. |
| Comments | All open discussions. |
| Changes | Your changes: write them, discuss them, implement them. |
| Commits | Changed files, commits, and the completed runs of the branch. |
| Terminals | Shells and agent CLIs in a worktree or a work stream. |
| Runs | Implementation runs and their progress. |
| Logs, History, Git | Application logs, the version history of a file, and branches and pull requests. |

## What you need

- **Docker.** Every implementation turn runs in a container.
- **An agent image** for each project. The project names it in `.synthesis/project.toml`. Reference Dockerfiles are in `docker/agent-images/`.
- **An AI API for discussions.** OpenRouter, Anthropic, OpenAI, or your own gateway.
- **A coding agent CLI.** Claude Code (subscription token or your own gateway) or Codex (your existing login).
- **A GitHub token** (optional), for pull requests, issues, and GitHub Projects on github.com.

Synthesis keeps all secrets in the keyring of your operating system.

## Build and run

You need Node.js with pnpm 10 or later, the Rust toolchain pinned in `src-tauri/rust-toolchain.toml`, and the Tauri system packages for your operating system (listed in `docs/development.md`).

```bash
pnpm install
pnpm tauri dev
```

To build an app bundle, run `task build`. All commands are in `docs/development.md`.

## More

- `docs/development.md`: build, test, and contribute.
- `docs/README.md`: the architecture and the known gaps.
- `docs/terminals.md`: the Terminals panel.
- `CLAUDE.md`: the rules for contributors and agents.

## License

Synthesis uses the [Business Source License 1.1](LICENSE). You can use it to build any software, also commercial software, and what you build is yours. You can read, change, and share its source. You cannot sell Synthesis, or a changed version of it, as a product or a service. Four years after each version is published, that version becomes available under the Apache License 2.0. Versions published before this license stay under the Apache License 2.0.
