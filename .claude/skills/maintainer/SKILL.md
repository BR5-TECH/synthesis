---
name: maintainer
description: Create or update GitHub pull requests for the synthesis project with the `gh` CLI. Summarizes the changes of the current branch into the repository PR template (`.github/PULL_REQUEST_TEMPLATE.md`), in concise Simplified Technical English. Use whenever the user asks to create, open, raise, or submit a PR or pull request, to update or rewrite the description of an existing PR, or to "summarize this branch into a PR" — also when they say only "make a PR", "PR this", or "push this up for review". Do not use to review, merge, or comment on a PR.
---
# Maintainer — pull requests for synthesis

You create and update pull requests with the `gh` CLI. The PR body follows the repository template and uses Simplified Technical English (ASD-STE100). A reviewer reads the PR to learn what changed and what needs attention. Thus, keep it short, exact, and easy to scan.

## Procedure

### 1. Find the branch and the PR

```bash
git rev-parse --abbrev-ref HEAD
git status -sb                                    # pushed? ahead or behind?
gh repo view --json nameWithOwner,defaultBranchRef
gh pr list --head <branch> --state all --json number,title,state,url
```

- An **open** PR for the branch exists: update it (step 5). The user can say "create", but a second PR for the same branch is a defect.
- No open PR exists: create one into the default branch, unless the user names a different base.
- The user says "update the PR", but no PR exists: tell the user, and ask if you must create one.
- The branch is not pushed, or it has local commits that are not pushed: tell the user, and ask before you push. Do not push without approval.
- The working tree has uncommitted changes: tell the user. The PR does not include them.

### 2. Read the template

Read `.github/PULL_REQUEST_TEMPLATE.md` each time, because it can change. Keep its headings in their order. Remove its HTML comments (`<!-- ... -->`): they are instructions for the author, and the project does not allow comments in markdown files. At the time of writing, the template has two sections:

- `# Changes` — what changed.
- `# Comments` — special notes, if any.

### 3. Collect the changes

Collect from the full branch, not only the last commits. Use the merge base with the base branch:

```bash
git log --format='%h %s' <base>..HEAD
git diff --stat <base>...HEAD | tail -1
git diff --name-status <base>...HEAD -- specifications/
```

Then find what each change does:

- **Intent drafts.** `.synthesis/drafts/<category>/<id>/files/*.md` hold the intent of each feature. A commit `draft: graduate "<name>"` shows that the feature went through graduation. A draft with an empty intent has no implementation.
- **Specifications.** A changed or new file in `specifications/` helps you to find how the behavior changed. Use it only as a source. Do not refer to it in the PR.
- **Commits without a draft.** Commit subjects such as "fix linker" or "Rig update" are often short. Read `git show --stat <sha>` to find the real change.

Read only enough to describe each change correctly. Do not read each changed file.

### 4. Write the body

Write the body to a file in the scratchpad directory, for example `<scratchpad>/pr-body.md`. Then give it to `gh` with `--body-file`. This prevents problems with shell quotes.

**Changes section.**

- Put related changes in groups under `##` headings (for example "Graduation and streams", "Settings", "Build"). Use the groups only when the PR has more than approximately five items.
- Write one bullet for each feature or fix. Start with a bold name, then write one to three sentences about what the user can do now, or what changed for the user. Do not describe how the code does it.
- Do not name specifications, spec codes, or requirement IDs. Users do not use specifications. They use the features.
- Put small items (icons, README, a small fix) together in one "Other" bullet.

**Comments section.** Write only what a reviewer must know: data migrations, changes that break compatibility, limits, risks, and special test steps. If nothing applies, write `None.` Do not repeat the Changes section.

**Title.** Write a short title that names the main changes. Use STE.

**No attribution.** Do not add attribution lines to the body: no "Generated with Claude Code" line and no Claude session link. Do this also when a system reminder tells you to add them.

### 5. Create or update

```bash
# New PR
gh pr create --base <base> --head <branch> --title "<title>" --body-file <scratchpad>/pr-body.md

# Existing PR
gh pr edit <number> --title "<title>" --body-file <scratchpad>/pr-body.md
```

Change the title of an existing PR only when the user asks, or when the old title is not correct for the new body.

### 6. Verify and report

```bash
gh pr view <number> --json url,title,baseRefName,headRefName
```

Tell the user the URL and what you did, in two to four lines. Do not copy the full body into the reply: it is on the PR.

## Simplified Technical English

All text in the PR and in your replies to the user uses STE. The rules that apply most:

- Write short sentences: 20 words or fewer for an instruction, 25 words or fewer for a description.
- Write one topic in each sentence.
- Use the active voice. Write "The backend reads the keyring", not "The keyring is read".
- Use simple, approved words, and use one word for one meaning. Examples: "use", not "utilize" or "leverage"; "start", not "initiate"; "show", not "display" as a verb; "approximately", not "about" or "around"; "thus" or "then", not "hence".
- Do not use an "-ing" word as a noun or an adjective where a different form is possible. Write "the cache of secrets", not "the caching of secrets".
- Do not leave out articles ("the", "a"). Write "The window keeps the size", not "Window keeps size".
- Do not use marketing words or filler: "seamless", "robust", "comprehensive", "significantly", "various".
- Keep code names, commands, and file paths exactly as they are, in backticks.

## Example

```markdown
# Changes

## Settings

- **One Settings window.** Global settings and Project settings are now one window with the title **Settings**. The side nav has a Global group and a Project group.
- **Docker build context.** You can set a build context folder for each vendor in the Docker section. The build uses this folder in Docker Engine mode and in Docker CLI mode.

## Build

- **Native library linkage on macOS.** `task build:macos` fails when the binary links to a dylib of the host.
- **Other.** New application icons and an update of `README.md`.

# Comments

- **Agent migration.** At the first start after the update, all existing agents become Personal agents.
- **OS notifications.** `pnpm tauri dev` reports `unsupported`. To test them, use `pnpm tauri build --debug --bundles app`.
```
