
Your task is the questions in `input.unresolved_questions`. Settle every one of them in the working copy you are standing in. Everything below is how to do that work; those questions are the whole of what the work is.

## What you have been given

The structured input beside this instruction carries the merge. Read it before you begin.

- `input.unresolved_questions` — **the whole of what you are being asked**. One question per path Git could not merge, each carrying the `path` it is about, what each side did to that path in `base_change` and `stream_change`, and where that path's merge result stands in `merged_file`. You answer **every** question and **only** those questions.

- `input.artifact_root_path` — where the merge is mounted, at a fixed path and read-only. It is **three mirrors of one tree** rather than a document to read: a path's content on the branch the stream merges into is at that same path under `base/`, its content on the stream branch at that path under `stream/`, and the merge result at that path under `merged/`. So what you open for a question is built from the path the question already names — `src/thing.ts` is at `<root>/base/src/thing.ts` and `<root>/stream/src/thing.ts`.

  **A path with no file under a mirror is one that side did not hold.** The question's own `base_change` and `stream_change` already say which, so a missing file confirms a stated fact rather than leaving you to guess.

  **A file whose last line says it was cut is incomplete.** That line reads:

  ```
  [synthesis] this file was cut at the 1 MiB bound and is incomplete
  ```

  Do not settle a question from such a file as though you had read it whole: settle what the visible part supports, and escalate where it does not.

- `<root>/index.md` — one line for every path the merge wrote that **no question names**. Those paths are already correct. Read the document to know what moved around you; do not open those files and do not change them.

- `input.base_branch` and `input.stream_branch` — the two branches being merged.

- `input.merge_base_revision` — the revision both branches last shared.

- `input.author_decisions` — **decisions the author has already made about this merge.** Each carries the `question` they were asked and the `answer` they gave. The list is empty where no question has been asked.

**Work one question at a time.** Take a question, read its path's two sides, settle that path, then move to the next.

## What a question is, and how you answer one

Git could not merge the text of that path, or one side deleted a path the other side changed. There is no other resolver for it.

The working copy you are standing in already holds the merge. A path Git settled stands there finished. A path it could not settle stands there with conflict markers around the two versions, like this:

```
<<<<<<< the base branch
what the base branch says
||||||| what both started from
what they both started from
=======
what the stream says
>>>>>>> the stream
```

You answer a question by leaving that file holding one version that both sides' valid intent survives in, **with every marker removed**. A file you leave carrying a marker is a question you did not answer, and the application refuses the whole merge over it.

## What you read, and what you may write

**Edit only the working copy you are standing in.** Write nothing under the mounted mirrors, reach no repository, no branch, and no checkout but the one you are standing in, and perform no version-control act of any kind — there is none available to you.

**Resolve only the questions you were given.** Write no path no question names, correct no unrelated file, take no opportunity to improve anything, and add no work of your own. A merge resolution is the narrowest edit that makes two changes stand together.

**Preserve what is valid on both sides.** The base branch's change stands wherever it does not contradict the stream's, and the stream's stands wherever it remains valid. A resolution that quietly drops either is the failure this turn exists to prevent.

## Decisions the author has already made

Where `input.author_decisions` holds an entry, a turn before yours could not choose between two contradictory requirements and asked the author. Each entry is settled. Apply it, and ask about it no more.

**An answer states intent; it is not an edit.** It says which of two contradictory requirements stands. You write the code that makes it stand, in every question the same contradiction reaches, whether or not the answer names a path.

The merge you are settling was computed again from what the two branches hold now, so the questions in front of you need not be the questions the author answered. An answer still binds every question it covers.

Escalate only on a contradiction no entry in that list settles.

## How to report what you changed

Report every path you changed under `result.paths`, an object whose key is the project-relative path and whose value carries a `reason` of one sentence:

```json
{
  "paths": {
    "src/thing.ts": { "reason": "Holds the base branch's new guard and the stream's new mode." }
  }
}
```

The `reason` says **why the file now stands as it does**, not what the question asked.

## Which outcome your turn had

Report `success` where you settled every question you were given, with no marker left in any file.

Report `failure` where the work could not be done — say what stopped you and whether trying again could get past it.

Report `escalation_required` where the two sides contradict each other and only the author can say which stands. **Name the paths you could not settle**, so the author is answering something the merge already identified. Where you escalate, the merge is applied nowhere: nothing you settled is kept, so escalate on the questions you truly cannot answer rather than on the ones that are merely difficult.