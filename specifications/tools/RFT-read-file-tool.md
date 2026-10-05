# Read file tool

**Spec code:** `RFT`

## Intent
The tool an agent reaches for once it knows which file it wants and needs what is actually written in it. It takes a project-relative path and returns that file's text — the whole of it by default, or a range of lines when the agent asks for one — so that a search which named a file can be followed by reading it. It is the retrieval half of the pair whose other half is `SPS-specification-search-tool.md`: ranking tells an agent where a decision was written down and this tool tells it what the decision was, and an agent that can do both follows a topic from a description to the text governing it without a human relaying files. It reads any text file the project holds rather than specifications alone, because an agent reasoning about a specification routinely needs the source, skill, or note that specification talks about. Out of scope: writing, creating, renaming, or deleting anything, this tool being read-only; reading anything outside the open project, which is refused whatever the application's filesystem allowlist otherwise permits; interpreting, summarising, or reformatting what was read; and discovering which path to ask for, which `SPS-specification-search-tool.md` and the skill tools answer.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`read_file` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Read a file from this project and get its text back. Use this when you already know which file you want — because a search named it, or because another file referred to it — and you need what is actually written in it. Give the path relative to the project root, exactly as it was reported to you. Returns the file's full text by default; pass `offset` and `limit` to read a range of lines instead, which is what you want for a file too large to be worth reading whole. This tool reads one file and returns it as written — it does not search for a file, does not summarise or interpret what it returns, and can neither reach outside this project nor change anything anywhere.

### Arguments
```
ReadFileArgs {
  path:    string,     // required; project-relative
  offset:  integer?,   // optional; 0-based first line, default 0
  limit:   integer?    // optional; line count, default the rest of the file
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `path` — *"The file to read, as a path relative to the project root. For example: 'src/main.ts'. A path outside the project cannot be read."*
- `offset` — *"The first line to return, counting from 0. Defaults to 0, the start of the file. Use it with `limit` to read part of a large file; an offset past the end of the file returns nothing."*
- `limit` — *"How many lines to return, starting at `offset`. Defaults to the rest of the file. A value below 1 is treated as 1, and a value reaching past the end returns the lines that exist."*

### Output
The file's text, as text (per TLC-FR-08). A document is returned unwrapped, so the model receives the file's own characters and nothing this tool composed.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank path** — kind `InvalidArgs`, retryable: *"The path must name a file to read. Give it relative to the project root, for example 'src/main.ts'."*
- **outside the project** — kind `PermissionDenied`, retryable: *"That path is outside this project, which is the only place this tool can read. Call again with a path relative to the project root."*
- **reached through a link** — kind `PermissionDenied`, retryable: *"That path reaches through a symbolic link, which this tool does not follow. Call again with the file's own path."*
- **no such file** — kind `NotFound`, retryable: *"No file exists at that path in this project. Check the path you were given, or search again for the file you want."*
- **path names a folder** — kind `InvalidArgs`, retryable: *"That path names a folder rather than a file. Call again with the path of a file inside it."*
- **not text** — kind `Other`, retryable: *"That file is not text and cannot be returned. It may be an image, an archive, or another binary format."*

## Functional requirements
 1. **RFT-FR-01** The tool exists as a `rig` portable tool named `read_file` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
 2. **RFT-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the three documented arguments with the documented parameter descriptions (per TLC-FR-05).
 3. **RFT-FR-03** `path` is interpreted as relative to the open project's active worktree root (`../core/WTC-worktree-context.md` WTC-FR-03), which is the form every tool in this group reports a path in, so a path a model was handed by another tool is one this tool accepts unchanged.
 4. **RFT-FR-04** The tool resolves `path` against that root and refuses with the outside-the-project refusal when the resolved path lies outside it. Resolution is lexical — `.` segments dropped and `..` segments collapsed against the segment preceding them, without consulting the disk — and containment is judged by where the path ends rather than by where it passed, so a path that climbs out of the root and back into it is read.
 5. **RFT-FR-05** Every read goes through `../core/FSA-filesystem-access.md`'s `read_text` on the instance belonging to the agent session the call is part of (FSA-FR-29), whose path-escape rejection (FSA-FR-10) and symlink refusal (FSA-FR-17) apply to the resolved path unchanged. That helper owns containment; the project-root bound of RFT-FR-04 narrows what is put to it and replaces none of what it does. Two bounds therefore hold at once and neither is the other's duplicate: the instance carries no reach to `app_data_dir()` for any caller to compose a path into, and the session temp directory it does carry — the one place a session's own scratch material lives — lies outside the project root and so is refused here, leaving this tool able to read the project and nothing else.
 6. **RFT-FR-06** With both `offset` and `limit` absent the whole file is returned, whatever its size. There is no size ceiling, no truncation, and no elision, so the text the model receives is the file's own text entire.
 7. **RFT-FR-07** `offset` is a 0-based line index, so `0` names the file's first line. It defaults to `0` when absent, and a negative value is clamped to `0` rather than refused (per TLC-FR-07).
 8. **RFT-FR-08** `limit` is a count of lines beginning at `offset`. It defaults to the rest of the file when absent, a value below `1` is clamped to `1` rather than refused (per TLC-FR-07), and a value reaching past the file's end returns the lines that exist rather than refusing.
 9. **RFT-FR-09** An `offset` at or beyond the file's last line is a success carrying empty text (per TLC-FR-12), the file holding no line at that position to return. An empty file answers the same way, there being nothing to distinguish and nothing lost by not distinguishing it.
10. **RFT-FR-10** The returned text is the file's own characters and nothing else — a document returned as text and wrapped in nothing (per TLC-FR-08). No path, no line number, no range marker, no count, and no sentence this tool composed is added to it, so a paged read differs from a whole-file read only in which of the file's characters it carries.
11. **RFT-FR-11** A line range carries each line's own terminator exactly as the file holds it, so the text of a range is precisely the span of the file those lines occupy and a file with `\r\n` endings reads back with them. A range ending at the file's last line carries that line's terminator only if the file has one.
12. **RFT-FR-12** A `path` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface.
13. **RFT-FR-13** A `path` naming nothing in the project is the retryable `NotFound` refusal. A model that misremembered a path corrects it on a further call, so telling it otherwise would cost it a file the project holds (per TLC-FR-11).
14. **RFT-FR-14** A `path` naming a folder is the retryable `InvalidArgs` folder refusal rather than a listing. This tool enumerates nothing, and a model told which mistake it made asks again for a file.
15. **RFT-FR-15** A file whose bytes do not decode as UTF-8 is the retryable not-text refusal. No part of its bytes reaches the model, the refusal message, or any log record.
16. **RFT-FR-16** A path the helper refuses for reaching through a symbolic link — at its final component or at any ancestor (`../core/FSA-filesystem-access.md` FSA-FR-17) — is the retryable link refusal, and the link's target is never opened or read.
17. **RFT-FR-17** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), there being no root to resolve a path against.
18. **RFT-FR-18** The tool answers from the filesystem as it stands and never blocks on background work (per TLC-FR-16). It consults no index, no scan, and no watcher, so a file created since the last pass is readable before any pass has observed it, and one deleted since produces the not-found refusal although the project tree still lists it.
19. **RFT-FR-19** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state. A file's own modification time is unchanged by being read.
20. **RFT-FR-20** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `path` the model asked for, and how many bytes it returned, and a `WARN` record naming the tool, the `path` the model asked for, and the reason when it refuses. The path is recorded as the model composed it — untrimmed and unresolved — because a record saying only that a read succeeded or was refused does not say which file it was about and so cannot be followed when a turn's reads are being traced. It is recorded bounded: a `path` longer than 512 characters is recorded as its first 512 followed by an ellipsis, so that no argument a model composes can push a record past `../core/LGC-logging.md`'s per-record size ceiling (LGC-FR-08) and cost it the tool and the reason, which matter more than the tail of a path no file could have. No record carries the `offset`, the `limit`, or any part of the file's contents — the contents are the project's own material.

## Non-functional requirements
- A call costs one whole-file read through the shared filesystem helper and no more; a line range is cut from what that read returned rather than by seeking, so the cost of a paged call is the cost of an unpaged one and paging buys context rather than I/O.
- The absence of a size ceiling (RFT-FR-06) puts the judgement about a large file where the model can act on it: the `offset` and `limit` pair is the tool's answer to a file too big to want whole, and a model that reads one entire did so having been told the pair exists.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one reads a project file it is given the path of, `RDT-read-draft-tool.md`'s reads a draft it is given the id of, `SPS-specification-search-tool.md`'s finds which specifications bear on a topic, `DST-draft-search-tool.md`'s finds which drafts do, and the three skill tools answer about skills.
- Nothing in this tool requires network access.
