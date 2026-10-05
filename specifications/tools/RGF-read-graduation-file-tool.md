# Read graduation file tool

**Spec code:** `RGF`

## Intent
The tool that lets a model-facing step of the graduation loop read what the run wrote. The manifest tells it which paths changed and what the application made of them, but what a specification now says is a question only its text can settle, and a step answering a question or writing a hand-off from a list of paths would be working from filenames. So this tool reads one file out of the run's own execution directory — the work stream's working copy — and returns its text. The execution directory is the point: it is a checkout of the project as it stood at the run's baseline with the agent's work on top of it, so one tool reaches both the specification the run produced and the specifications it has to be consistent with, and neither is confused with whatever the author happens to have open. It exists as its own capability rather than as a reuse of `RFT-read-file-tool.md` for exactly that reason — that tool reads the **active worktree** (RFT-FR-03), which during a graduation is very often a different branch, a different worktree, or a different project entirely, and a loop given it would read a plausible-looking wrong tree and never know. Out of scope: reading for a phase that dispatches an execution agent, which reads its own working copy; writing, creating, renaming, or deleting anything, this tool being read-only; reading outside the run's execution directory, which is refused whatever the application's filesystem allowlist otherwise permits; listing or searching a directory, this tool taking a path it was given; and deciding what the text means, which is the loop's judgement.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to the graduation loop alone (`../ai/GRL-graduation-loop.md` GRL-FR-EKXT) and has no Tauri command, event, or frontend reach.

### The tool
`read_graduation_file` — a `rig` portable tool whose constructor binds it to **one graduation run** and to an `../core/FSA-filesystem-access.md` instance whose single allowed root is that run's execution directory (per FSA-FR-18). The run is fixed before the model composes an argument, so there is no run identifier among its parameters and no way for a model to read out of a run it is not judging.

### The description
The fixed text the model reads (per TLC-FR-05):

> Read a file from the working copy this graduation is being written in, and get its text back. Use it on the paths the change set names, to see whether what was written actually answers the prompt, and on any other file in the project to check that it is consistent with what is already there. Give the path relative to the project root, exactly as the change set reports it. Returns the file's full text by default; pass `offset` and `limit` to read a range of lines instead, which is what you want for a file too large to be worth reading whole. This working copy already holds every change made so far, so what you read is what would be published. This tool reads one file and returns it as written — it does not list a folder, does not search, does not summarise what it returns, and can neither reach outside this working copy nor change anything anywhere.

### Arguments
```
ReadGraduationFileArgs {
  path:    string,     // required; project-relative
  offset:  integer?,   // optional; 0-based first line, default 0
  limit:   integer?    // optional; line count, default the rest of the file
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `path` — *"The file to read, as a path relative to the project root. For example: 'specifications/ui/ABC-thing.md'. A path outside this working copy cannot be read."*
- `offset` — *"The first line to return, counting from 0. Defaults to 0, the start of the file. Use it with `limit` to read part of a large file; an offset past the end of the file returns nothing."*
- `limit` — *"How many lines to return, starting at `offset`. Defaults to the rest of the file. A value below 1 is treated as 1, and a value reaching past the end returns the lines that exist."*

### Output
The file's text, as text (per TLC-FR-08). A document is returned unwrapped, so the model receives the file's own characters and nothing this tool composed.

### Refusals
- **blank path** — kind `InvalidArgs`, retryable: *"The path must name a file to read. Give it relative to the project root, for example 'specifications/ui/ABC-thing.md'."*
- **outside the working copy** — kind `PermissionDenied`, retryable: *"That path is outside the working copy this graduation is being written in, which is the only place this tool can read. Call again with a path relative to the project root."*
- **reached through a link** — kind `PermissionDenied`, retryable: *"That path reaches through a symbolic link, which this tool does not follow. Call again with the file's own path."*
- **no such file** — kind `NotFound`, retryable: *"No file exists at that path in this working copy. Check the path against the change set, or read a file you know is there."*
- **path names a folder** — kind `InvalidArgs`, retryable: *"That path names a folder rather than a file. Call again with the path of a file inside it."*
- **not text** — kind `Other`, retryable: *"That file is not text and cannot be returned. It may be an image, an archive, or another binary format."*
- **working copy unavailable** — kind `NotFound`, not retryable: *"The working copy this graduation is being written in cannot be reached, so nothing can be read from it. This will keep failing; do not retry."*

## Functional requirements
1. **RGF-FR-01** The tool exists as a `rig` portable tool named `read_graduation_file` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool. It is **read-only in full** (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state, so nothing a graduation judges can be changed by the judging.
2. **RGF-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the three documented arguments with the documented parameter descriptions (per TLC-FR-05). Two instances constructed for two different runs return byte-identical definitions, the run being a fact of construction rather than of the schema.
3. **RGF-FR-03** The tool is attached to the graduation loop's **model-facing steps** and to nothing else — the hand-off phase, the clarification judgement, and the requirements amendment judgement (per `../ai/GRL-graduation-loop.md` GRL-FR-EKXT). No phase that dispatches an execution turn is lent it. No conversational turn is sent it (per `../ai/CVL-conversation-loop.md` CVL-FR-08), no Tauri command registers it, and `src/**` cannot reach it.
4. **RGF-FR-04** `path` is interpreted as relative to the root of the run's **execution directory** — the work stream's working copy for a work turn, and the run's throwaway review checkout for a review turn — resolved through `../core/GRD-graduation.md`'s `execution_directory` rather than composed by this tool. That root is the project root as the run sees it, so a path the run reports as changed is a path this tool accepts unchanged.
5. **RGF-FR-05** The tool never reads the project's **active worktree**. The instance its constructor holds carries the run's execution directory as its single allowed root and carries no reach to the active worktree, to `app_data_dir()`, or to any session temp directory, so a graduation judged while the author is on another branch, in another worktree, or in another project reads the run's own tree and nothing else. This is the distinction from `RFT-read-file-tool.md`, whose root is the active worktree (RFT-FR-03), and the two tools are never attached to one agent.
6. **RGF-FR-06** The tool resolves `path` against that root and refuses with the outside-the-working-copy refusal when the resolved path lies outside it. Resolution is lexical — `.` segments dropped and `..` segments collapsed against the segment preceding them, without consulting the disk — and containment is judged by where the path ends rather than by where it passed, so a path that climbs out of the root and back into it is read.
7. **RGF-FR-07** Every read goes through `../core/FSA-filesystem-access.md`'s `read_text` on that instance, whose path-escape rejection (FSA-FR-10) and symlink refusal (FSA-FR-17) apply to the resolved path unchanged. That helper owns containment and the bound of RGF-FR-06 narrows what is put to it; two bounds therefore hold at once and neither is the other's duplicate.
8. **RGF-FR-08** The tool reads **any text file the execution directory holds**, not the manifest's paths alone. A specification the change set does not touch, a template, and a document the new specification must be consistent with are all readable, because a question about what the run must satisfy routinely turns on what the surrounding specifications already say. The manifest names what changed; this tool names what may be read, and the two sets are different on purpose.
9. **RGF-FR-09** With both `offset` and `limit` absent the whole file is returned, whatever its size. There is no size ceiling, no truncation, and no elision, so the text the model receives is the file's own text entire.
10. **RGF-FR-10** `offset` is a 0-based line index, defaulting to `0` when absent, with a negative value clamped to `0` rather than refused (per TLC-FR-07). `limit` is a count of lines beginning at `offset`, defaulting to the rest of the file, with a value below `1` clamped to `1` and a value reaching past the file's end returning the lines that exist.
11. **RGF-FR-11** An `offset` at or beyond the file's last line is a success carrying empty text (per TLC-FR-12), the file holding no line at that position to return. An empty file answers the same way.
12. **RGF-FR-12** The returned text is the file's own characters and nothing else — a document returned as text and wrapped in nothing (per TLC-FR-08). No path, no line number, no range marker, no count, and no sentence this tool composed is added to it, and each line carries its own terminator exactly as the file holds it.
13. **RGF-FR-13** A `path` that is empty once trimmed, one naming nothing in the working copy, one naming a folder, and one whose bytes are not text are each the retryable refusal of the contract surface. A misremembered path is corrected on a further call, so every one of them says a further call could succeed (per TLC-FR-11).
14. **RGF-FR-14** An execution directory that has been removed or cannot be reached is the not-retryable **working copy unavailable** refusal, because no path the model could compose would change it. `../core/GRD-graduation.md` GRD-FR-CYIB is what the run does about it; this tool's part is to say plainly that reading is over rather than to report a missing file for every path in turn.
15. **RGF-FR-15** The tool holds no state between calls beyond the run and the instance its constructor bound, and it is cheap enough to construct per judgement (per TLC-FR-15). An instance never outlives the run it was built for, and an instance built for one run can reach no other run's execution directory.
16. **RGF-FR-16** Every call emits through `../core/LGC-logging.md`'s internal API under the `ai` and `backend` domains together: an `INFO` record when the call ends, naming the tool, the run id, the project-relative `path`, and the outcome, and a `WARN` record when it refuses, naming the tool, the run id, the `path`, and the reason. The run id and the `path` are the argument-adjacent values these records name, a record about a file read being unfollowable without saying which run and which file (per TLC-FR-14); the `path` is bounded before it is recorded so no value a model composes can grow a record past `../core/LGC-logging.md` LGC-FR-08's per-record ceiling. No record carries the file's contents or any part of them.

## Non-functional requirements
- The read is one file read through an instance already constructed, so a judgement that reads eight files costs eight reads and nothing else. Nothing here walks a directory or builds an index.
- The tool reaches no network and resolves no credential (per TLC-FR-18), and no credential is reachable through its arguments, its output, its refusals, or its log records.
- The tool answers from the execution directory as it stands and waits on no scan, no index, and no watcher (per TLC-FR-16), which is what lets it be called immediately after an execution turn has finished writing.
- A file the tool returns is the file that would be published if the run were approved, because the execution directory is where publication reads from. There is no second copy for the two to disagree about.
