# Embeddings index

**Spec code:** `EMB`

## Intent
The optional vector index that improves the ranking of specifications and documents. It keeps embeddings of their chunks in a per-project SQLite store, and it fuses vector ranks with the BM25 ranks of `BMI-bm25-indexing.md` by Reciprocal Rank Fusion. The `search_specifications` and `search_documents` agent tools and the Semantic mode of the universal search bar use this fusion. The vectors of specifications are branch-aware: a root layer from the Git objects of the default branch, plus the difference of the active branch, so a worktree switch embeds only what its branch changed. When embeddings are off or not ready, every search gets the BM25 result, as without this module. Out of scope: BM25 itself, which stays on at all times; embeddings of drafts, notes, skills, and other files; the settings surfaces, which are `../ui/GLS-global-settings.md`'s and `../ui/SET-project-settings.md`'s.

## Functional requirements
1. **EMB-FR-WQGF** Embeddings **work** for a project only when the Global switch is on and the project's mode is not `off`. The mode `use_global` follows the Global switch. A project with mode `on` does nothing while the Global switch is off.
   - *Why:* the Global switch is a master switch, so a project set to `on` never starts a model download by itself.
2. **EMB-FR-VXIS** The Global switch is off on a fresh install. While embeddings work for no project and the Global switch is off, this module downloads no model, loads no model, and embeds nothing.
3. **EMB-FR-UUML** When embeddings do not work for the open project, no embedding job runs for it, every search for it uses BM25 only, and its store stays on disk unchanged.
4. **EMB-FR-DWJB** The Global switch and the selected model persist in the user-global store of `GSS-global-settings-storage.md` (GSS-FR-MBJV). The project mode persists in the per-project slot of that store (GSS-FR-DSUG). This module writes neither value into a repository or into `.synthesis/`.
5. **EMB-FR-JFOV** `save_embeddings_settings(settings)` persists the Global switch and the model, and returns the stored settings. An unknown model id is refused with `unknown_model`, and nothing is stored. Turning the Global switch off stops every running job at once, and every later search uses BM25 only.
6. **EMB-FR-CTXC** `save_project_embeddings_mode(mode)` persists the mode of the open project and returns it. A change that makes embeddings work starts the project-open check (EMB-FR-JIJH). A change that stops them stops every job of the project. With no project open, the command fails with `no_project_open`.
7. **EMB-FR-MIDG** `list_embedding_models` returns every text embedding model that FastEmbed supports in this build. The four recommended models come first, in this order: BGE-small-en-v1.5, all-MiniLM-L6-v2, EmbeddingGemma 300M, BGE-M3. Each of them has `recommended = true`. The other models follow, sorted by name.
8. **EMB-FR-XTLI** The default model is BGE-small-en-v1.5. A store with no selected model reads as the default.
9. **EMB-FR-MHHY** Inference uses the Candle backend of FastEmbed when Candle can run the selected model, and ONNX Runtime when it cannot. Each model entry names the backend that runs it. Both backends are compiled into the application.
   - *Why:* in FastEmbed, Candle runs only some models, and the four recommended models run on ONNX Runtime.
9. **EMB-FR-QQER** When a model defines a query prefix or a passage prefix, the module puts that prefix before the query text or the chunk text that it embeds. The prefix changes no chunk ID and no content hash. A change of the prefixes that the application uses is a change of the store schema version (EMB-FR-UJLS).
10. **EMB-FR-OJRK** A chunk text that is longer than the input limit of the model is truncated to that limit before it is embedded. The chunk, its chunk ID, and its BM25 text do not change.
11. **EMB-FR-NNOK** Models download from HuggingFace into a cache under `app_data_dir()/models/`. A model that the cache holds whole loads with no network access. A download starts only while the Global switch is on and the selected model is not in the cache.
11. **EMB-FR-KUFM** The cache folder is always `app_data_dir()/models/`. The environment variables `HF_HOME`, `HF_ENDPOINT`, and `FASTEMBED_CACHE_DIR` do not change where a model downloads from, where it is stored, or where it loads from. When `app_data_dir()` is not available, the download fails with a reason, and the module writes no file to a temporary folder.
11. **EMB-FR-MRDV** A process that the application starts gets `HF_HOME`, `HF_ENDPOINT`, and `FASTEMBED_CACHE_DIR` with the values that the application itself started with, or without the variable when the application started without it.
   - *Why:* an agent launch passes variables by name from the application process, so a user's own hub settings must reach it unchanged.
12. **EMB-FR-LYKW** A download reports its progress in bytes, in the status (EMB-FR-ZFGT) and as an operation (EMB-FR-HDPR). A failed download sets the model state to `failed` with a reason, writes an `ERROR` record, and leaves no partial file that a later load could read as a cached model. The reason carries no query string of a URL.
13. **EMB-FR-KEGS** `retry_embeddings_model_download` starts the download again when the model state is `failed`. Nothing retries a failed download automatically. While the model state is `failed`, vector search is not ready for any project.
14. **EMB-FR-RFNF** Each project has one vector store: the file `embeddings.sqlite` in the repository store of `RMS-repository-machine-storage.md` (RMS-FR-MVDU). Every worktree of the project uses that file. No part of the store is inside a worktree or under `.synthesis/`.
15. **EMB-FR-KNBG** The store uses `sqlite-vec`. It keeps one vector for each distinct chunk content hash, the SHA-256 of the chunk text. A chunk whose content hash already has a vector in the store is never embedded again, in any layer.
16. **EMB-FR-CZND** A layer entry is a chunk ID (`BMI-bm25-indexing.md` BMI-FR-OEQM) with the content hash of that chunk, or with a deletion marker. A vector serves a chunk only when the hash of its entry equals the hash of the chunk text that the BM25 index holds now under the same chunk ID.
17. **EMB-FR-UJLS** The store records the model id and the store schema version. When either differs from the selected model or from the schema version of the application, the store is **stale**. The module then deletes the store and builds all layers again in the background, with no confirmation.
18. **EMB-FR-HBLQ** The pass that removes the last layer entry that refers to a vector also deletes that vector.
19. **EMB-FR-RFBJ** A store that cannot be opened, read, or written is **unavailable**. Every search then uses BM25 only, the status names the store as unavailable, and the module writes an `ERROR` record. The next trigger tries to open the store again. A store that another application process holds busy is unavailable for that attempt.
20. **EMB-FR-VFPV** The module embeds the chunks of specifications, the files whose resolved artifact type is `spec` (`ASC-artifact-scanning.md` ASC-FR-06), and the chunks of the available documents of `DCL-documents-collection.md`. Drafts, notes, skills, and all other files have no vectors.
21. **EMB-FR-IRQA** A chunk here is a chunk of `BMI-bm25-indexing.md`, made by the same rules (BMI-FR-05, BMI-FR-06, BMI-FR-FGGU) and named by the same chunk ID (BMI-FR-OEQM). One chunk therefore has the same ID in both indexes.
22. **EMB-FR-HDFV** The **root branch** is the default branch of `CHC-changes.md` CHC-FR-13. Its **tip** is the commit of the local branch of that name. When no local branch has that name, the tip is the remote-tracking branch of the primary remote. A fetch that moves only a remote-tracking ref does not move a tip that a local branch gives.
23. **EMB-FR-LGLI** The **root layer** holds the specification chunks of the root tip commit. The module reads them from the Git objects of that commit (`GTC-git.md` GTC-FR-SHLL, GTC-FR-GUSU), with no checkout. It classifies each file by the rules of ASC-FR-06, with the assignments that the tree of that commit holds. The layer records the tip commit id.
24. **EMB-FR-FALJ** The root layer holds only the files below the project folder in the tree of the tip commit, under their project-relative paths. A chunk of the root layer and the same chunk of the working copy therefore have the same chunk ID.
25. **EMB-FR-DQVX** When the root tip differs from the tip commit id that the root layer records, the module updates the root layer by content hash. It embeds only the chunks whose hash has no vector, removes the entries of removed chunks, and records the new tip commit id.
26. **EMB-FR-DCXI** The **branch layer** of the active worktree is built from its working copy, uncommitted edits included. It holds each chunk that is new or whose content hash differs from the root chunk with the same ID, and a deletion marker for each root chunk that the working copy does not have.
27. **EMB-FR-MBJV** A branch layer never holds a chunk that equals the root chunk with the same ID. An edit that makes a branch chunk equal to the root chunk again removes that chunk from the branch layer. When the active worktree is on the root branch, its layer holds only what its working copy changes against the root tip.
28. **EMB-FR-DSUG** The **current vector index** is the root layer without the root chunks that the branch layer replaces or marks as deleted, plus the chunks of the branch layer. Vector search of the `spec` index sees this view only, and it sees no other branch.
29. **EMB-FR-NTMA** The module builds a branch layer only for the active worktree while it is on a named branch. On a detached HEAD there is no branch layer, and vector search of the `spec` index uses the root layer only.
30. **EMB-FR-USBA** Each branch layer records its branch name and the root tip that it was compared with. When its worktree becomes active again after the root tip moved, the module rebuilds its difference. At each full evaluation, the module deletes the layer of a branch that no longer exists in the repository or that no worktree has checked out.
31. **EMB-FR-MGML** When the project is not inside a Git repository, the module builds one layer from the working copy. There is then no root branch, no root layer, and no branch. This layer counts as the branch layer.
32. **EMB-FR-PYCS** The documents have one layer. A change of the active worktree, of the branch, or of the root tip does not change it (`DCL-documents-collection.md` DCL-FR-XHSJ).
33. **EMB-FR-HWRQ** A specification change reaches this module on the same paths as it reaches `BMI-bm25-indexing.md`: structural changes from the project watcher (`ASC-artifact-scanning.md` ASC-FR-10), content modifications on the channel of ASC-FR-15, and type changes (BMI-FR-17). A batch with only `.gitignore`-excluded paths starts no pass (ASC-FR-RSGA). This module mounts no watcher of its own.
34. **EMB-FR-OAJQ** A file whose resolved type becomes `spec` enters the active branch layer with its chunks. A file that loses the `spec` type leaves it. A change written by Git, such as a pull, a checkout, or a merge, is a filesystem change like any other.
35. **EMB-FR-IBXK** A change of the Documents collection reaches this module on the channel of `DCL-documents-collection.md` DCL-FR-QGLH. An added document is embedded. A document with a new `revision` is compared by chunk hash. A document that left the collection or became unavailable loses its entries.
36. **EMB-FR-WWMW** The module collects the changes that arrive in the 2 seconds after the first change of a burst, then runs one incremental pass for all of them.
37. **EMB-FR-BZZT** An incremental pass compares the content hash of each changed chunk with the stored entry of the same chunk ID. It embeds only the hashes that have no vector, writes the new entries, and removes the entries of removed chunks. For the branch layer, it compares with the root chunk of the same ID (EMB-FR-MBJV).
38. **EMB-FR-EAEA** A pass writes all its store changes in one transaction. A pass that fails leaves the stored entries and vectors as they were, terminates its operation as `failed`, writes an `ERROR` record, and raises no error in any search. The next trigger tries again.
39. **EMB-FR-OVPR** **First indexing.** When embeddings start to work for a project whose store holds no layer, the module builds in the background the root layer, the branch layer of the active worktree, and the documents layer.
40. **EMB-FR-JIJH** **Project-open check.** When embeddings work for an opening project, the module keeps a store that is not stale and, in the background, compares the root layer by its recorded tip commit id, the active branch layer by chunk hash with the working copy, and the documents layer by chunk hash with the collection. It embeds only the changed chunks.
41. **EMB-FR-NHQG** When the active worktree changes (`WTC-worktree-context.md` WTC-FR-08), or when the branch or the HEAD of the active worktree changes, the module evaluates the layers again for the new worktree and branch. It builds or updates the branch layer by difference only.
41. **EMB-FR-HTIQ** The module finds a change of the branch or the HEAD of the active worktree from the Git-metadata watch of `CHC-changes.md` CHC-FR-16 (CHC-FR-VHAA). A change made outside the application, such as `git switch` or `git checkout --detach` in a terminal, therefore starts the evaluation of EMB-FR-NHQG, also when no working-tree file changes.
42. **EMB-FR-VQSK** The module checks whether the root tip moved at project open, when the active worktree or its branch changes, when the Git-metadata watch of `CHC-changes.md` CHC-FR-16 reports a change, and when the main window gains focus. When the tip moved, it updates the root layer (EMB-FR-DQVX) and then rebuilds the difference of the active branch layer.
43. **EMB-FR-UTWM** Changes made while the application was closed, or while embeddings did not work for the project, are found by the project-open check (EMB-FR-JIJH). The module keeps no journal of missed events.
44. **EMB-FR-TGCL** **Model switch.** When `save_embeddings_settings` stores a different model while the Global switch is on, the module downloads the model when the cache does not hold it. It then deletes the vectors of the previous model and the layers of every branch that is not active, and builds the root layer, the active branch layer, and the documents layer again.
45. **EMB-FR-UTCB** After a model switch, the store of a project that was not open is stale (EMB-FR-UJLS), and it is built again when that project opens. A branch layer that a model switch deleted is built again, by difference only, when its worktree becomes active.
46. **EMB-FR-HRYV** `start_reindex(scope)` with scope `current_branch` removes and embeds again the active branch layer and the documents layer, and keeps the root layer. With scope `whole_trace` it removes and embeds again the root layer, the active branch layer, and the documents layer.
47. **EMB-FR-QNWF** On a detached HEAD, `current_branch` embeds the documents layer again only. In a project outside Git, the working-copy layer is the branch layer. `start_reindex` fails with `embeddings_not_working` when embeddings do not work for the open project, and with `no_project_open` when no project is open.
48. **EMB-FR-RXCL** A Reindex embeds again every chunk of the layers in its scope. It does not reuse a vector that only those layers refer to, and it keeps every vector that a kept layer refers to.
49. **EMB-FR-SWRV** Only one job runs at a time for a project. A model switch, a stale-store rebuild, or a Reindex cancels a running job and runs in its place. A change that arrives during a job is applied by an incremental pass after the job ends.
50. **EMB-FR-BLHF** Vector search is **ready** for the open project only when embeddings work for it, the model is loaded, the store is available, and none of these jobs runs: first indexing, project-open check, worktree or branch evaluation, root tip update, model switch, stale-store rebuild, or Reindex.
51. **EMB-FR-YLBN** An incremental pass does not make vector search not ready. While it runs, a chunk whose current content hash has no vector gets its BM25 rank only (EMB-FR-CZND).
52. **EMB-FR-BWTI** `hybrid_search(index, query, limit)` accepts the `spec` and `documents` indexes. When vector search is not ready, it returns exactly what `search([index], query, limit)` of `BMI-bm25-indexing.md` (BMI-FR-10) returns.
53. **EMB-FR-NJYA** When vector search is ready, `hybrid_search` takes the ranked BM25 chunk list and the ranked vector chunk list of the current view, each of a bounded length. It fuses them by Reciprocal Rank Fusion: a chunk's score is the sum of 1/(60 + rank) over the lists that hold it, with ranks from 1.
54. **EMB-FR-LPKD** `hybrid_search` returns the `limit` chunks with the highest fused score, as `ChunkHit` values whose `score` is the fused score. A chunk that only the vector list holds gets its text and fields from the BM25 index. A vector hit that fails the hash rule of EMB-FR-CZND is dropped.
55. **EMB-FR-IFWD** Chunks with equal fused scores are ordered by their BM25 rank, then by chunk ID. The same query against the same store and index state gives the same order.
56. **EMB-FR-HWTF** `hybrid_search` never returns an error. A failure in the vector part, such as the query embedding or a store read, makes that call return the BM25-only result and writes a `WARN` record. It never waits for a job to end.
57. **EMB-FR-JLRR** A query embedding has priority over the batches of a job. It waits for the batch that is running, and for no other batch.
58. **EMB-FR-HDPR** Every job and every model download registers one operation with `PRG-progress-reporting.md` under `kind = "embed"` (PRG-FR-11), and it terminates that operation when the work ends, whatever the outcome. The operation becomes determinate when its total is known. The status bar shows it on the terms of `../ui/STB-status-bar.md` STB-FR-NAEX.
59. **EMB-FR-WJCW** The operation label names the work. A model download is "Downloading embedding model…". The job kind `first_indexing` is "Embedding specifications…". `open_check`, `branch_update`, and `root_update` are "Checking embeddings…". `incremental` is "Updating embeddings…". `model_switch` is "Switching embedding model…". `stale_rebuild` is "Rebuilding embeddings…". `reindex` is "Reindexing embeddings…".
60. **EMB-FR-ZFGT** `get_embeddings_status` returns the status of the model and of the open project. The event `"embeddings status changed"` carries the same status each time a field changes, at most a few times per second.
61. **EMB-FR-OLDK** Each job emits through `LGC-logging.md` under the `backend` domain: an `INFO` record when it starts and when it ends, with its counts of compared, embedded, and removed chunks and its duration; a `WARN` record for a skipped file or document; and an `ERROR` record when it fails.
62. **EMB-FR-EYGO** No record carries a chunk text, a query, a vector, or a document path. A record names a document by its id.
63. **EMB-FR-RMZH** Closing a project stops every job of that project, keeps its store on disk, and closes the store file.
64. **EMB-FR-VVBY** This module reads the working copy through `FSA-filesystem-access.md` and the documents through `DCL-documents-collection.md` (DCL-FR-GLUS). It writes only the store, the model cache, and its settings through `GSS-global-settings-storage.md`. It changes no file of a worktree, no Git object, no ref, and no index of Git.

## Contract surface
The module owns the vector store, the model cache, the embedding jobs, and the commands and event below. The UI consumers are `../ui/GLS-global-settings.md` (the Global Indexing section) and `../ui/SET-project-settings.md` (the Project Indexing section).

### Tauri commands
- `"load embeddings settings"` → `load_embeddings_settings` → `EmbeddingsSettings`.
- `"save embeddings settings"` → `save_embeddings_settings(settings)` → `EmbeddingsSettings`, or the typed error `unknown_model`.
- `"list embedding models"` → `list_embedding_models` → `EmbeddingModel[]`.
- `"retry embeddings model download"` → `retry_embeddings_model_download` → `EmbeddingsStatus`. A model state other than `failed` makes it a no-op.
- `"load project embeddings mode"` → `load_project_embeddings_mode` → `ProjectEmbeddingsMode`, or `no_project_open`.
- `"save project embeddings mode"` → `save_project_embeddings_mode(mode)` → `ProjectEmbeddingsMode`, or `no_project_open`.
- `"get embeddings status"` → `get_embeddings_status` → `EmbeddingsStatus`.
- `"start reindex (scope)"` → `start_reindex(scope)` → `EmbeddingsStatus`, or `no_project_open` or `embeddings_not_working`. It returns when the job is queued, before any chunk is embedded.

### Events
- `"embeddings status changed"` → `EmbeddingsStatus` (EMB-FR-ZFGT).

### Internal Rust API
No UI consumer and no Tauri command.

- `hybrid_search(index, query, limit)` → `[ChunkHit]` of `BMI-bm25-indexing.md`, for `index ∈ { spec, documents }` (EMB-FR-BWTI). Consumed by `../tools/SPS-specification-search-tool.md`, `../tools/SDT-search-documents-tool.md`, and `SCC-search.md`.
- The triggers of EMB-FR-HWRQ, EMB-FR-IBXK, EMB-FR-NHQG, and EMB-FR-VQSK, called next to the matching triggers of `BMI-bm25-indexing.md`, `DCL-documents-collection.md`, and `WTC-worktree-context.md`.

### Payload shapes
```
EmbeddingsSettings {
  enabled,          // the Global switch; false when unset
  model             // a model id of list_embedding_models; BGE-small-en-v1.5 when unset
}

EmbeddingModel {
  id,               // the FastEmbed model id
  name,             // display name
  dimension,        // vector length
  backend,          // "candle" | "onnx"
  recommended,      // true for the four models of EMB-FR-MIDG
  default           // true for BGE-small-en-v1.5 only
}

ProjectEmbeddingsMode = "use_global" | "on" | "off"

ReindexScope = "current_branch" | "whole_trace"

EmbeddingsStatus {
  enabled,
  model,
  model_state,      // "not_downloaded" | "downloading" | "loading" | "ready" | "failed"
  download?,        // { completed_bytes, total_bytes? } while downloading
  model_error?,     // the reason, when model_state is "failed"
  project?: {       // absent when no project is open
    mode,           // ProjectEmbeddingsMode
    working,        // EMB-FR-WQGF
    state,          // "off" | "preparing" | "ready" | "updating" | "unavailable"
    job?,           // { kind, completed?, total? }; kind ∈ "first_indexing" | "open_check"
                    // | "branch_update" | "root_update" | "model_switch" | "stale_rebuild"
                    // | "reindex" | "incremental"
    git,            // "repository" | "none"
    branch?,        // the active branch; absent on a detached HEAD or outside Git
    detached,
    root_branch?,
    root_tip?,      // the commit id that the root layer records
    spec_chunks,    // entries of the current vector index
    document_chunks,
    last_error?     // the reason of the last failed job
  }
}
```

`state` is `ready` when vector search is ready (EMB-FR-BLHF), `updating` while an incremental pass runs on a ready index, `preparing` while a job of EMB-FR-BLHF runs, `unavailable` when the store is unavailable (EMB-FR-RFBJ), and `off` when embeddings do not work for the project.

## Non-functional requirements
- Search stays as fast as BM25 alone when vector search is not ready, because the fallback is one call to the BM25 index.
- The corpus of a project is small, hundreds to some thousands of chunks, so a vector query may scan the current view in full.
- Jobs run on one worker thread, off the thread that serves the UI, in small batches. No Tauri command waits for a job.
- The bounded lengths of the two lists of EMB-FR-NJYA and the batch size of EMB-FR-JLRR are implementation choices.
- The only network access of this module is the model download from HuggingFace.
- A model of the cache is shared by every project. A store is shared by every worktree of one repository.
