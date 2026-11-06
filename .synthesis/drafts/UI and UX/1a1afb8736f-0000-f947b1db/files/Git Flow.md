## Intent

Git Flow experience should be improved in the IDE, providing a frictionless experience for a User working on large projects using GitHub or Jira as a project management system.

## User journey

### Work on a new feature

- User already has a feature in task tracking system (i.e. a Feature inside GitHub Project)
- User opens IDE and uses worktree/branch selector as his starting point.
- User creates a draft and works on it as long as it takes, including collaboration with AI agents.
- User might create as many drafts as it needed to fulfill the feature, depending on its size, scope or complexity.
- User publishes the drafts (i.e. Publish to GitHub)


- Eventually user starts graduation of his drafts in one or more work streams, preserving association.
- Work streams are eventually merged into the working branch.
- At the bottom of the Changes panel user sees a read-only stacked list of completed graduation runs that were merged into the current branch.
- Once the user considers the feature is done he clicks a `Create PR` button in Changes panel.
- In the `Create PR` dialog user has an option to provide title and description manually, and a button to generate the title/description by using AI agent to generate it out of the diff.
- User clicks the button. The app starts a new kind of graduation run (a PR summary run) in the background. The run has a queue and a project slot like every other run, and it shows in the Runs panel. The run uses the agentic integration with the PR-maintenance model override.
- Progress bar shows the progress, and `Create PR` dialog also reflects that there’s a background work, blocking the “Generate” button from double clicks.
- Eventually the title/description are ready and populated into the corresponding fields of the `Create PR` dialog.
- Before that, each draft run already wrote its own title and description when its review passed. The first time user opens `Create PR` for a head branch with empty title and description, the dialog combines these run summaries into the fields. User edits the result, or clicks Generate to replace it with a fresh title/description made from the diff.


- `Create PR` has an option to associate the PR with one or more features. The dialog lists the open issues of the publication repository whose issue Type is one of the configured parent issue Types (per `GHP-github-publication.md` GHP-FR-KVRH, GHP-FR-FTMC).
- `Create PR` understands the difference between a regular PR and a stacked PR. A PR is stacked when its base is the branch of another PR. The dialog detects this from the base branch and states it.
- Once PR is created, it’s automatically linked to the tasks (GitHub issues) that were published from the drafts associated with the branch's work, if they were published.
- Once PR is created, it’s automatically linked to the features that user picked up during PR creation, if they were picked. The link rule is in Requirements.
- User will be able to start working on the next Feature starting the process from the beginning.

## Requirements

- The existing GitHub API integration (`GTC-git.md`) must be extended to link a created PR to issues. The app first calls a GitHub API that links the PR to each issue, so the link shows in the Development sidebar of the PR. The implementer must verify which call exists (for example a GraphQL mutation); no such call is confirmed in this project. If no call exists, or GitHub refuses it, the app falls back to closing keywords: it adds a bullet list to the PR description, one line per task or feature, for example `* Resolves #123`. A link failure never fails the PR creation. The dialog reports which issues were linked by which way. Closing keywords close an issue only when the PR merges into the default branch, so a stacked PR closes nothing; the dialog says so for a stacked PR. 
- GitHub stacks must be supported as well as classic GitHub flow.
- Agentic AI (e.g. Claude Code) must be used to generate PR summary with an expectation that User’s templates, skills, agent instructions etc are in place to explain how the PR summary must look like.
- File modifications, graduations, etc are never applied directly to project’s main branch unless user **explicitly** confirms his intent. GitHub protected branches must be respected.
- Create PR dialog should be adjusted to support markdown in description.
- Create PR dialog should be increased in side to fit more significant PR descriptions.
- Agentic AI must be instructed to use structured output with JSON that generates title/description. Description is expected to support markdown format.
- A draft run records a **run summary** (a title and a markdown description) in its durable run record, when its review answers `ready` and the run completes. The reviewer prompt in `resources/prompts` is extended to ask for it, with the same rules as the PR summary prompt (user's templates, skills and agent instructions, JSON structured output). A review that returns no valid summary does not fail the run or change its verdict; the run just holds no summary. Merge, pull and direct runs write no summary.
- On the first open of `Create PR` for a head branch, when title and description are both empty and no saved dialog state exists, the dialog builds them from the run summaries of the completed draft runs merged into the head branch. Runs with no summary are skipped. When no run has a summary, the fields stay empty and the dialog does not start a run.
- The title is the title of the first such run in merge order. User can edit it.
- The description is built this way. When the agentic integration is configured, the app starts a PR summary run (same run kind, queue, slot and model override as Generate) that gets the run summaries as input and returns one description in the same format. When it is not configured, or that run fails, the description holds one section per run in merge order, each with the run's title as heading. The dialog blocks **Generate** while this run is queued or working, and shows the same progress.
- **Generate** works as designed above: it makes a new title/description pair from the diff and fills the fields with it. Combining from run summaries never runs again on its own for a branch whose dialog state exists.
- User can open/close the `Create PR` dialog without resetting the progress of the run. The full dialog state (title, description, base branch, draft toggle, selected features, generation run and result) is kept per head branch for the life of the application session, and a successful submit clears it. This replaces the rule that closing the dialog discards everything (`CPR-create-pull-request.md` CPR-FR-SSQI).
- The PR summary run is a new run kind of `GRD-graduation.md`. It has no draft and no stream, and it runs on the head branch's working copy without changing a file. The dialog blocks **Generate** while a run for that head branch is queued or working.
- The app never writes to the main branch without an explicit confirmation dialog. This covers every write of the app to that branch: draft creation, file save, commit, push, merge, graduation and direct run. A branch is main when it is the repository default branch or when GitHub reports it as protected.
- For the default branch that GitHub does not protect, the user can confirm the write. For a branch that GitHub protects, the dialog is a hard block: it offers no confirm action and routes the user to a new branch and `Create PR`. A failed protection read must not allow the write silently; the dialog states that protection is unknown and asks for confirmation.

## Expected Deliverables

- IDE adjustments to support new flows.
- Create PR dialog visual adjustments.
- Setting dialog Agentic AI model selector is expanded to include an override for PR maintenance. This is a new turn kind in the per-turn-kind model and effort overrides (`AIC-agentic-integrations.md` AIC-FR-10, `EAC-execute-agent-cli.md` EAC-FR-IRRD, `AII-ai-integrations.md`). It follows the default model when no override is set.
- New prompt in `resources/prompts` to support creation of PR summary with respect to the skills and project rules user might have in the repository.
- Adjusted reviewer prompt in `resources/prompts`, a run summary field in the graduation run record, and the combine logic in the `Create PR` dialog.
- Confirmation dialog and protected-branch check for writes to the main branch, in the frontend and in the backend operations that write.
- Backend operation to link a PR to issues, with the body fallback.
- Read-only list of completed graduation runs in the Changes panel.
- Updates to these specifications: `CPR-create-pull-request.md` (controls, markdown, size, per-branch state, features, link result, generate), `GTC-git.md` (create PR, link operation, protection read), `GRD-graduation.md`, `GRU-graduation-runs.md` and `RUN-runs.md` (new run kind), `GRL-graduation-loop.md` (review turn returns the run summary), `CHG-changes.md` (completed runs list), `DRP-drafts-panel.md` and `DRS-draft-storage.md` (draft creation on main), `GSD-graduation-start-dialog.md` and `WSS-work-stream-selector.md` (main confirmation), `AIC-agentic-integrations.md`, `EAC-execute-agent-cli.md` and `AII-ai-integrations.md` (turn kind and override). 

## Out of Scope:

- Jira integration is out of scope.
- GitHub polling we have is out of scope.