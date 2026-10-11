## Intent

Users defining the Dockerfile for agentic tools (Claude Code we use in graduation) often need an option to define a custom build context. We should provide them with such an option.

## User journey

- User opens new unified global settings window
- User navigates to Docker build in project area
- User can provide path to Dockerfile manually as today.
- User can use native os file picker to specify a Dockerfile (opens by default in the project folder)


- User can type a path to a build context (optional). An empty field means the project root, as today.
- User can use the native OS folder picker to select a build context. It opens in the project folder, or in the current context folder when one is set. The user can select a folder outside the project, and the selection is kept as made (for example, a folder of certificates).
- User clicks Build. The build uses the Dockerfile and the build context that the user specified, in Bollard / Docker Engine mode and in Docker CLI mode.

## Requirements

- The Dockerfile file-picker button and the Build context folder-picker button are each rendered inside their own single-line text field (trailing end), not as separate buttons beside it. Each button has an accessible label and works with the keyboard.
- The empty Build context field shows the placeholder "Project root (default)".
- Look and feel of the unified app should not suffer and must stay aligned with a single settings window and IDE as a whole.
- Each vendor entry (Claude Code, Codex, OpenCode) holds its own optional Build context. It is saved at once with the other fields of the entry (no dirty state), and it is committed in `.synthesis/project.toml` with the entry.
- A Build context path is either project-relative or absolute. A relative path resolves against the project root. An absolute path is machine-specific and not portable in the committed file. A path outside the project is allowed. The path must name an existing folder. If it does not, the save is refused with an inline message in the tab that names the problem, and the entry stays unchanged. A folder chosen in the picker goes through the same check.
- The Dockerfile path rules do not change: project-relative, inside the project, a regular file (PSS-FR-24).
- "Project root" is context-aware. When the project uses a worktree at the time of the build, the active worktree root is the root, for the empty default and for relative paths.
- A Dockerfile may lie outside the Build context. The Docker CLI mode needs no change, because `docker build -f <Dockerfile> <context>` accepts it. No automatic fall back to the Docker CLI exists, so the Bollard mode keeps PSS-FR-26 ("the Docker CLI path takes no part") and GSS-FR-35 (the mode is the user's choice).
- Bollard / Docker Engine mode, Dockerfile inside the context: send the context archive and set the build option `dockerfile` to the Dockerfile path relative to the context. Add no extra file.
- Bollard / Docker Engine mode, Dockerfile outside the context: add the Dockerfile to the context archive under the unique name `.pss-Dockerfile` and set `dockerfile` to that name. The Dockerfile content stays byte-for-byte unchanged, so every instruction (`COPY`, `ADD`, `RUN --mount`, multi-stage, `ARG`, heredocs, `# syntax=`) works as written. The build sees only the context, as with any Docker build, so project files are not available to it.
- `.dockerignore` is read from the context root only, as the Docker CLI does. A Dockerfile-specific ignore file (`<Dockerfile>.dockerignore`) is out of scope.
- A build failure that Docker reports is shown in the build result region of the tab, as today (PSS-FR-28).

## Specifications to change

- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-22 (entry gains the optional Build context), PSS-FR-24 or a new requirement next to it (Build context validation and its typed refusals), PSS-FR-26 (context choice, worktree-aware root, Dockerfile outside the context in Bollard mode).
- `specifications/ui/SET-project-settings.md`: SET-FR-21 (new control in the tab order), SET-FR-22 (folder picker, outside selection accepted, inline refusals), SET-FR-25 (Build uses the chosen context).
- Both specifications use the current Project settings window. The draft "Single settings window" changes where the Docker section lives, so the wording of the SET requirements must follow whichever of the two drafts graduates first.