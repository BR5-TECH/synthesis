# Synthesis

**A desktop IDE where you plan a change with AI agents, then let a coding agent build it.**

```
Write a change  →  Discuss it with agents  →  Implement it  →  Review and merge
```

You describe what you want in plain words. Agents ask questions and propose edits until the change is clear. Then a coding agent, such as Claude Code or Codex, does the work in a Docker container on its own Git branch, and a second agent reviews it. You read the result and merge it.

Synthesis does not decide for you, and it does not replace your coding agent or your skills. It is one place to plan the work, watch it, and review it. You bring your own AI keys and subscriptions.

> **Early development.** Version 0.1.0, for one person on one computer. There are no releases yet: you build it from source.

## Quick start

1. **Install** Docker, Node.js with pnpm 10+, Rust, and the [Tauri system packages](docs/development.md) for your OS.
2. **Run** Synthesis:
   ```bash
   pnpm install
   pnpm tauri dev
   ```
3. **Connect your AI.** In **Settings**, fill in **AI API** (for discussions) and **Agentic AI** (for Claude Code or Codex).
4. **Open** a Git repository, then click **+ Change** in the **Changes** panel to write your first change.

## How it works

### 1. Write a change

A **change** is one Markdown prompt: a feature, a fix, or a new requirement. Synthesis keeps it in your repository and commits it to Git.

### 2. Discuss it with agents

Mention an agent in a comment, for example `@critic`, or write `@all`. Agents answer, ask questions, and propose **edits** to the prompt. Nothing changes until you accept an edit.

Agents in a discussion cannot read or run your code. They see only what Synthesis gives them.

### 3. Implement it

Click **Implement**. Synthesis starts an **implementation run**: a coding agent works in a container, then a fresh agent session reviews the work. The run repeats until the review says `ready`, and then Synthesis commits the result.

Follow the run in **Runs → Implementation**. If the agent needs you, the run waits for your answer there.

### 4. Review and merge

Each run works on a **work stream**, a branch that Synthesis owns. See what it changed in the **Commits** panel, then merge it into your base branch. If Git finds a conflict, an agent resolves it and another agent reviews the fix. Open a pull request from the **Git** panel.

## More you can do

- **GitHub.** Publish a change as an issue, or claim a "Ready" task from a GitHub Project and implement it as a change.
- **OpenSpec.** In a project that uses [OpenSpec](https://github.com/Fission-AI/OpenSpec), you can first propose an **OpenSpec change** and then apply it.
- **Terminals.** Open a shell or an agent CLI in any worktree or work stream. See [docs/terminals.md](docs/terminals.md).

<details>
<summary><b>The panels</b></summary>

| Panel | What it is for |
| --- | --- |
| Project | The files and artifacts of the project. |
| Documents | Reference documents that agents can search. |
| Notes | Short notes to yourself. |
| Comments | All open discussions. |
| Changes | Your changes: write them, discuss them, implement them. |
| Commits | Changed files, commits, and the completed runs of the branch. |
| Terminals | Shells and agent CLIs. |
| Runs | Implementation runs and their progress. |
| Logs, History, Git | Application logs, the history of a file, and branches and pull requests. |

</details>

<details>
<summary><b>What you need</b></summary>

- **Docker.** Every implementation turn runs in a container.
- **An agent image** for each project, named in `.synthesis/project.toml`. Reference Dockerfiles are in `docker/agent-images/`.
- **An AI API for discussions:** OpenRouter, Anthropic, OpenAI, or your own gateway.
- **A coding agent CLI:** Claude Code (a subscription token or your own gateway) or Codex (your existing login).
- **A GitHub token** (optional), for pull requests, issues, and GitHub Projects on github.com.

Synthesis keeps all secrets in the keyring of your operating system.

</details>

<details>
<summary><b>Build an app bundle</b></summary>

You need the Rust toolchain pinned in `src-tauri/rust-toolchain.toml` and the [Task](https://taskfile.dev) runner. Then run `task build`. All commands are in [docs/development.md](docs/development.md).

</details>

## Learn more

- [docs/development.md](docs/development.md): build, test, and contribute.
- [docs/README.md](docs/README.md): the architecture and the known gaps.
- [CLAUDE.md](CLAUDE.md): the rules for contributors and agents.

## License

[Business Source License 1.1](LICENSE). Use Synthesis to build any software, also commercial software: what you build is yours. You can read, change, and share the source, but you cannot sell Synthesis, or a changed version of it, as a product or a service. Each version becomes Apache License 2.0 four years after it is published. Versions published before this license stay under Apache License 2.0.
