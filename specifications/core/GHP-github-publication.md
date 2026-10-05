# GitHub publication

**Spec code:** `GHP`

## Intent
The backend that publishes a draft's prompt as a **GitHub issue**, so that work prepared in the New Artifact tab can be picked up by an agent that runs outside this application. It resolves which configured Git remote the issue goes to, checks that the repository accepts new issues and that the project's GitHub token can read and write them, writes an **attempt record** before the first GitHub request, creates the issue through GitHub's REST API, and appends one record to the draft's **append-only publication history**. The issue is published as a **root issue** or as a **sub-issue** of an open issue in the same repository, with an optional GitHub issue **Type** and **milestone**; the author's choices are saved in the attempt record before the first GitHub mutation. Publication is a fact about a draft's metadata rather than about its prompt: it never edits the prompt, never removes a draft, and never changes local graduation. Out of scope: the remote agent that reads the issue and works on it; labels, assignees, comments, and issue state; media attachment and local-asset upload, which v1 refuses rather than performs; hosts other than `github.com`; adding an issue to a GitHub Project, the publication having no Project selector of its own; minting or storing a token, which is `GTS-github-token-storage.md`'s; and the storage layout of the publication store, which is `DRS-draft-storage.md`'s.

## Functional requirements
1. **GHP-FR-QJNV** Publication targets `github.com` alone and reaches it through a programmatic HTTPS client against GitHub's REST API, authenticated with the secret the open project resolves through `GTS-github-token-storage.md` (GTS-FR-13). The `gh` CLI is neither required nor invoked, and no operation here shells out to any external program.
2. **GHP-FR-WKDE** `list_publication_remotes(draft_id)` enumerates **every configured Git remote of the project** and **mutates nothing on GitHub**. It classifies each remote, resolves which one the next attempt would use, and reports the origin of that choice. It creates no issue, edits no issue, and writes no attempt record.
3. **GHP-FR-BXTU** A remote is `kind = "github"` when its URL canonicalizes to a `github.com` repository, in either the HTTPS or the SSH form, and `kind = "other"` otherwise. Canonicalization drops the scheme, the credentials, the port, a trailing `.git`, and a trailing slash, and lowercases the host, so one repository reached by two URL forms canonicalizes to one value.
4. **GHP-FR-MZPR** A GitHub remote is **eligible** only when three facts hold: the repository record is readable, the repository accepts new issues, and the project-resolved token may write issues to it. A remote that fails one carries the eligibility value naming which — `issues_inaccessible`, `issues_disabled`, `issues_create_forbidden`, or `token_unavailable` — together with the exact reason as displayable text.
5. **GHP-FR-TKBW** A repository **accepts new issues** only when its repository record reports Issues turned on, and reports the repository neither archived nor disabled. A repository that does not carries `issues_disabled`, whose displayable reason names the repository setting rather than the token.
6. **GHP-FR-RQDV** The token **may write issues** unless the granted scopes it reports exclude it: a classic token must report `repo`, or `public_repo` for a public repository. A token that reports no scopes at all is fine-grained and is not inferred against. Code-write permission on the repository is never required.
    - *Why:* Anybody who can read a repository with Issues on may open an issue in it, so a push-permission test refuses every repository the author does not own.
7. **GHP-FR-LTAC** A remote whose `kind` is `"other"` is never eligible and carries `not_github` as its reason. It is reported rather than omitted, so the author reads why a remote they configured cannot receive an issue.
8. **GHP-FR-HVQG** A **persisted choice** is used only when a remote of that name is still enumerated, its canonicalized URL still matches the persisted one, and that remote is eligible. Otherwise it is ignored for this attempt and is not rewritten, so a remote rename or URL change invalidates it.
9. **GHP-FR-XAUP** Where no persisted choice applies and **exactly one** configured remote exists and is eligible, that remote is used automatically. Where **two or more** configured remotes exist, the caller chooses one from the whole enumeration.
10. **GHP-FR-NDSB** `origin` states how the reported `selection` was reached: `automatic` for the single eligible remote of GHP-FR-XAUP, `persisted` for a stored choice that still applies, `attempt_only` for a remote the caller named without asking to persist it, and `none` where no remote resolves.
11. **GHP-FR-ZRFP** Where the project has no configured remote, no GitHub remote, or no eligible remote, `selection` is null, `origin` is `none`, and publication is refused with the typed error naming which case holds: `no_remote_configured`, `no_github_remote`, `token_unavailable`, `issues_inaccessible`, `issues_create_forbidden`, or `issues_disabled`.
12. **GHP-FR-WNJC** Where more than one refusal holds across the project's GitHub remotes, the refusal names the first of `token_unavailable`, `issues_inaccessible`, `issues_create_forbidden`, `issues_disabled` that holds.
    - *Why:* The first three are conditions of the token and stop every remote; `issues_disabled` is a condition of one repository, and naming it first sends the author to the wrong settings page.
13. **GHP-FR-CVYK** `publish_draft_to_github(draft_id, remote_name, persist_remote, publication_choice)` re-resolves the named remote before it publishes: the remote must still be enumerated and still eligible at the moment of the call. One that is not is refused with its own typed error and publishes nothing. A persisted choice is validated the same way on every attempt.
14. **GHP-FR-PWXA** `persist_remote` true writes the chosen remote's name and canonicalized URL into project-local settings (per `PSS-project-settings-storage.md` PSS-FR-TQFB) and applies to every later attempt in this project. `persist_remote` false writes nothing and applies the choice to the current attempt alone.
15. **GHP-FR-GJEO** Only a draft whose status is `active`, `published`, or `graduated` may be published. An `archived` draft is refused with `draft_archived`, whose displayable reason states that the draft must be restored before publication.
16. **GHP-FR-BKLT** A GitHub-shadow draft (per `DRS-draft-storage.md` DRS-FR-WFLY) is refused by every publication operation with `draft_github_shadow`, and nothing is written or requested. `get_draft_publication` reports it as not publishable with that reason.
16. **GHP-FR-SRDL** A draft a non-terminal graduation run holds is refused with `draft_locked_by_graduation` (per `DRS-draft-storage.md` DRS-FR-19), and nothing is written or requested.
17. **GHP-FR-AKUM** Before the first GitHub request of an attempt, publication reads the **saved prompt's images** through `DAS-draft-assets.md`'s `read_prompt_images` (DAS-FR-22) and refuses with `local_assets_unsupported` where any image reference is not an absolute `http` or `https` URL. The error carries every affected reference. No issue and no remote asset is created.
    - *Why:* v1 uploads nothing, so an issue carrying a relative path would render a broken image to whoever picks the work up.
18. **GHP-FR-DTVW** An image reference that is already an absolute `http` or `https` URL is carried into the issue body unchanged and blocks nothing.
19. **GHP-FR-FQIZ** An attempt's **marker** is an opaque value generated once, unique across the project, and stable for the whole life of that attempt. It is written into the issue body on its own line as `<!-- synthesis-publication-marker: <opaque-marker> -->` and is searchable in an issue body by exact match. It is never written into the draft's prompt.
20. **GHP-FR-XOBH** The issue **title** is exactly the draft's current name. The issue **body** is the complete current prompt followed by the marker line of GHP-FR-FQIZ and nothing else.
21. **GHP-FR-RUYT** Before the first GitHub request, the attempt record is written **atomically** to the draft's publication store (per `DRS-draft-storage.md` DRS-FR-EJBM). It holds the draft id, the marker, the remote, the repository, the state `open`, and the publication choice of GHP-FR-ATCH. The store stays in the checkout that wrote it and no commit names it (per `DRS-draft-storage.md` DRS-FR-WBTA).
22. **GHP-FR-KZAP** A draft holds **at most one non-terminal attempt**. `publish_draft_to_github` against a draft that already holds one is refused with `attempt_in_progress` and starts nothing; the standing attempt is continued through `retry_draft_publication` or ended through `cancel_draft_publication_attempt`.
23. **GHP-FR-VNCS** A publication that fails at any step **keeps the draft's previous status**, appends no history record, and leaves the attempt record standing in a non-terminal state, so the attempt stays recoverable. The typed error naming the exact failure is returned.
24. **GHP-FR-OWLB** `retry_draft_publication(draft_id)` reuses the standing attempt's marker, remote, repository, and publication choice, and **searches the target repository's issues for that exact marker before creating anything**. It always sends the draft's **latest** name as the title and its latest prompt as the body.
    - *Why:* An issue creation whose response was lost still created the issue, so a retry that did not search would post the same work twice.
25. **GHP-FR-IEQC** Where the search finds no issue carrying the marker, the retry creates the issue on the terms of GHP-FR-XOBH. Where the search cannot answer — GitHub is unreachable, or the search is refused — the retry returns a typed error and creates nothing.
26. **GHP-FR-TMBK** Where the search finds an issue carrying the marker whose title and body equal the latest title and body and whose parent, Type, and milestone match the saved choice (GHP-FR-CMPR), the retry **reuses that issue** and creates no second one, then completes the attempt on the terms of GHP-FR-JAWD.
27. **GHP-FR-HRUN** Where the search finds an issue carrying the marker whose title, body, parent, Type, or milestone differs from the latest title and body and the saved choice, the retry returns `recovery_required` carrying that issue's number and URL and the list of what differs, and moves the attempt to `awaiting_choice`. It edits and creates nothing on its own.
28. **GHP-FR-YPGL** `resolve_draft_publication_conflict(draft_id, "update_existing")` reconciles the found issue to the latest title, body, and saved choice (GHP-FR-RCNL), keeps the attempt's marker, and completes the attempt with **one** history record. `"publish_new"` ends the attempt without a history record and starts a **deliberate re-publication**: a new attempt, a new marker, the saved choice, and a new issue.
29. **GHP-FR-EBSA** Cancelling the recovery choice — `cancel_draft_publication_conflict(draft_id)` — returns the attempt to `open`, appends no history record, and edits nothing on GitHub, so the attempt stays recoverable.
30. **GHP-FR-JAWD** A successful publication appends **one** record to the draft's history carrying the provider `github`, the repository owner and name, the issue number, the issue URL, the publication instant in RFC 3339 UTC, the attempt's marker, and the attempt's publication choice (GHP-FR-HSCH); and clears the attempt, both in **one atomic write** of the publication store.
31. **GHP-FR-UZMX** The history is **append-only**. No operation here updates, reorders, replaces, or deletes a record already in it. The **current publication** is the newest record; every earlier record stays readable and keeps naming the issue it named.
32. **GHP-FR-QLDF** A **deliberate re-publication** of a draft that already has history always starts a new attempt with a new marker and creates a new issue. It never reuses a marker from a completed record and never edits an issue an earlier record names.
33. **GHP-FR-BSYH** A successful publication of a draft whose status is `active` sets that status to `published` **only after the issue exists**, through `DRS-draft-storage.md`'s `set_draft_published` (DRS-FR-VKQO). A draft whose status is `graduated` keeps `graduated`, and a draft already `published` stays `published`; in both cases the history record is appended just the same.
34. **GHP-FR-NAXT** `cancel_draft_publication_attempt(draft_id)` abandons the standing attempt: the attempt record is cleared, no history record is appended, and nothing on GitHub is created, edited, or deleted. An issue an abandoned attempt already created stays on GitHub and is not recorded.
35. **GHP-FR-CWTG** `get_draft_publication(draft_id)` returns the draft's current record, its whole history newest-first, its standing attempt or null, and its current eligibility. It is a **read**: it contacts GitHub only for the eligibility checks of GHP-FR-MZPR and writes nothing.
36. **GHP-FR-ZFPI** On application restart every standing attempt is read back from the publication store and stays recoverable. A draft holding one is never left permanently unpublishable: an `open` attempt offers retry and an `awaiting_choice` attempt offers the recovery choice of GHP-FR-YPGL, and neither blocks abandoning it.
37. **GHP-FR-MJTB** `open_publication_issue(draft_id, url)` opens the OS default browser at that URL and opens no page inside the application. It refuses a URL that neither a record of that draft's history nor the draft's GitHub-shadow issue link (per `DRS-draft-storage.md` DRS-FR-XDWS) holds, and one that is not a `github.com` issue address, so this operation cannot open an arbitrary address.
38. **GHP-FR-DHXK** No error payload, metadata view, stored record, log line, or event of this module carries a token secret or a remote URL with an embedded credential, on the terms `GTC-git.md` GTC-FR-11 sets for Git output.
39. **GHP-FR-YDAN** `resolve_publication_repository()` resolves the project's publication remote on the terms of GHP-FR-HVQG and GHP-FR-XAUP without a draft, and returns the remote name and the repository owner and name, or the refusal code of GHP-FR-ZRFP. It writes nothing and creates nothing on GitHub.
39. **GHP-FR-PVOA** The typed error vocabulary is exactly `no_project_open`, `draft_not_found`, `draft_archived`, `draft_locked_by_graduation`, `local_assets_unsupported`, `no_remote_configured`, `no_github_remote`, `issues_inaccessible`, `issues_disabled`, `issues_create_forbidden`, `token_unavailable`, `attempt_in_progress`, `no_attempt`, `github_unreachable`, `issue_create_failed`, `issue_update_failed`, `publication_store_write_failed`, `draft_github_shadow`, `parent_issues_unreadable`, `issue_types_unreadable`, `milestones_unreadable`, `parent_issue_unavailable`, `issue_type_unavailable`, `milestone_unavailable`, `invalid_publication_choice`, `sub_issue_link_failed`, and `invalid_publication_settings`. Each carries displayable text stating the exact failure.

40. **GHP-FR-KVRH** The **GitHub publication settings** are three project-public values stored through `PSS-project-settings-storage.md` (PSS-FR-HWBG): the **parent issue Types** (default `Feature`), the **sub-issue Type** (default `Task`), and the **sub-issue milestone policy** (default `inherit_parent`). `get_github_publication_settings()` returns them with defaults applied to every unset value.
41. **GHP-FR-NQWX** `set_github_publication_settings(parent_issue_types, sub_issue_type, sub_issue_milestone_policy)` persists the three values and returns them. It refuses an empty Type list, an empty Type name, and a policy outside `inherit_parent`, `no_milestone`, and `author_selected` with `invalid_publication_settings`, and writes nothing. It never checks a Type against GitHub, so a saved Type that GitHub does not list stays saved. A failed write returns `publication_store_write_failed`.
42. **GHP-FR-PTYL** `list_github_issue_types()` resolves the publication repository on the terms of GHP-FR-YDAN, reads the issue Types of that repository's owner, and returns the Type list of GHP-FR-DZLB. A repository that does not resolve returns its GHP-FR-ZRFP refusal. It mutates nothing on GitHub.
43. **GHP-FR-MDLD** `load_publication_metadata(draft_id, remote_name)` re-resolves the named remote on the terms of GHP-FR-CVYK and returns the repository, the settings of GHP-FR-KVRH, the **parent list**, the **Type list**, the **milestone list**, and the resolved sub-issue Type. It mutates nothing on GitHub and writes no attempt record.
44. **GHP-FR-DZLB** Each of the three lists is `{ state, items, error_code, error }` with `state` either `loaded` or `failed`. One list that fails to read does not fail the command and does not change the other two lists. A `loaded` list may hold no item.
45. **GHP-FR-FTMC** A **parent issue** is an open issue of the selected publication repository that is not a pull request and whose Type equals a configured parent issue Type, compared without regard to case. A row carries the number, title, Type, milestone or null, and URL. The request names each Type in the owner's spelling where the owner lists it.
46. **GHP-FR-YSPJ** The **Type list** holds the names of the issue Types that GitHub reports for the repository's owner. An owner that GitHub reports no Types for, including a user account, gives a `loaded` list with no item. Any other failed read gives `failed` with `issue_types_unreadable`.
47. **GHP-FR-GHEA** The **milestone list** holds the open milestones of the selected repository, each with its number and title. A failed read gives `failed` with `milestones_unreadable`. A failed parent read gives `failed` with `parent_issues_unreadable`.
48. **GHP-FR-UXOT** No failed metadata read refuses root publication. A root issue publishes without a Type when the Type list holds none or failed, and without a milestone when the milestone list holds none or failed.
49. **GHP-FR-BWNI** The **publication choice** passed to `publish_draft_to_github` is `{ parent_issue_number, issue_type, milestone_number }`, each value a number, a string, or null. A null `parent_issue_number` requests a **root issue**. A number requests a **sub-issue** of that issue in the selected publication repository.
50. **GHP-FR-LCKZ** For a root issue, a named `issue_type` must equal a Type of the Type list and a named `milestone_number` must be an open milestone of the milestone list. Otherwise the call is refused with `issue_type_unavailable` or `milestone_unavailable`, and nothing is written or requested to create an issue.
51. **GHP-FR-RMVQ** For a sub-issue, the call reads the parent and refuses with `parent_issue_unavailable` unless the parent exists, is open, is not a pull request, and has a configured parent issue Type. A sub-issue choice that names an `issue_type` is refused with `invalid_publication_choice`. Nothing is written for a refused call.
    - *Why:* The sub-issue Type is set in Project settings alone, so the choice never carries one.
52. **GHP-FR-ETJD** The **sub-issue Type** is the configured sub-issue Type, resolved to the spelling that the Type list holds. Where the Type list does not hold it, or failed, the sub-issue is published without a Type and the metadata view reports the configured Type as unavailable.
53. **GHP-FR-AZPF** The **sub-issue milestone policy** resolves the milestone. `inherit_parent` uses the parent's current milestone, or none where the parent has none. `no_milestone` uses none. `author_selected` uses the open milestone named by `milestone_number`, or none where it is null. A `milestone_number` under another policy is refused with `invalid_publication_choice`.
54. **GHP-FR-ATCH** Before the first GitHub mutation, the attempt record holds the **publication choice**: root or sub-issue; the parent repository and issue number; the root Type or the resolved sub-issue Type; the sub-issue milestone policy; and the resolved milestone number and title. Every read that resolves the choice happens before the attempt record is written.
55. **GHP-FR-OHGY** A create request carries the draft's title and body and, where the saved choice names them, the Type name and the milestone number. For a sub-issue, the created issue is then linked to its parent through GitHub's sub-issue relationship, the parent being in the selected publication repository.
56. **GHP-FR-SWKU** A link that fails returns `sub_issue_link_failed`, appends no history record, and leaves the attempt standing. A create that GitHub rejects for its Type or milestone returns `issue_create_failed` and leaves the attempt standing.
57. **GHP-FR-IBXN** A retry, a recovery answer, and a restart reuse the saved choice exactly. They do not read the settings, the Type list, or the milestone list again, and they never reopen the choice.
57. **GHP-FR-PWJG** Before a create or a link, the retry reads the saved parent. It returns `parent_issue_unavailable` where the parent is missing, closed, a pull request, or in another repository than the attempt's. The attempt stays `open`, and no root issue is published in its place.
58. **GHP-FR-CMPR** An issue found by marker **matches** the saved choice when its parent equals the saved parent, or it has no parent for a root choice; when its Type equals the saved Type, if one is saved; and when its milestone number equals the saved milestone, if one is saved. A value the choice does not name is not compared.
59. **GHP-FR-RCNL** **Update existing** edits the found issue to the latest title and body and to the saved choice: it sets a saved Type and milestone that differ, links a saved parent that differs and replaces any other parent, and removes the parent of a root choice. A failed link or unlink returns `sub_issue_link_failed` and leaves the attempt in `awaiting_choice`.
60. **GHP-FR-HSCH** A history record carries the **publication choice** of its attempt: whether the issue was published as a root issue or as a sub-issue, the parent repository and issue number, the Type name, the milestone policy, and the milestone number and title. A record written before this choice existed carries none and reads as a root issue.
61. **GHP-FR-PLQE** A root publication with no Type and no milestone sends a create request with the title and body alone, so its wire shape is the shape of a publication before parent, Type, and milestone existed.
62. **GHP-FR-CDVT** An attempt record that carries no publication choice reads as a root choice with no Type and no milestone. A choice read back after an application restart equals the choice written, so the retry of GHP-FR-IBXN needs no state held in memory.
63. **GHP-FR-NDWB** `resolve_draft_publication_conflict(draft_id, "publish_new")` replaces the standing attempt with the new one in one atomic write. A failed write leaves the old attempt and its saved choice standing.

## Contract surface
Every operation is a Tauri command invoked by `../ui/NAW-new-artifact.md`. The quoted name is the canonical operation name and matches that spec's "Delegated to backend (abstract)" line byte-for-byte.

### Record shapes
The persisted shapes — `DraftPublicationRecord`, `DraftPublicationAttempt`, and the store holding them — are `DRS-draft-storage.md`'s (DRS-FR-EJBM) and are not restated here. Both carry the `PublicationChoice` below as `choice`.

```
PublicationRemoteEligibility =
  | "eligible" | "not_github" | "issues_inaccessible"
  | "issues_disabled" | "issues_create_forbidden" | "token_unavailable"

PublicationRemote {
  name,                 // the configured Git remote's name
  url,                  // canonicalized remote URL (GHP-FR-BXTU)
  kind,                 // "github" | "other"
  repository_owner,     // null when kind is "other"
  repository_name,      // null when kind is "other"
  eligibility,          // PublicationRemoteEligibility
  reason                // displayable text; null only when eligible
}

PublicationRemoteResolution {
  remotes: [PublicationRemote],   // every configured remote, in enumeration order
  selection,                      // remote name the next attempt uses, or null
  origin,                         // "automatic" | "persisted" | "attempt_only" | "none"
  persisted_choice                // { name, url } | null (GHP-FR-PWXA)
}

PublicationEligibility {
  publishable,          // bool
  reason_code,          // one typed error value of GHP-FR-PVOA; null when publishable
  reason,               // displayable text; null when publishable
  local_assets          // the offending image references (GHP-FR-AKUM); empty otherwise
}

DraftPublicationView {
  current,              // newest DraftPublicationRecord, or null
  history,              // every DraftPublicationRecord, newest first
  attempt,              // the standing DraftPublicationAttempt, or null
  eligibility           // PublicationEligibility
}

PublicationOutcome =
  | { kind: "published", record }
  | { kind: "recovery_required", issue_number, issue_url, marker, mismatches }
                                 // mismatches: [ "title" | "body" | "parent" | "type" | "milestone" ]

PublicationChoiceInput {          // sent by the caller (GHP-FR-BWNI)
  parent_issue_number,            // number | null; null requests a root issue
  issue_type,                     // string | null; a root issue only
  milestone_number                // number | null
}

MilestonePolicy = "inherit_parent" | "no_milestone" | "author_selected"

GithubPublicationSettings {       // GHP-FR-KVRH
  parent_issue_types,             // [string], at least one
  sub_issue_type,                 // string
  sub_issue_milestone_policy      // MilestonePolicy
}

MetadataList<T> {                 // GHP-FR-DZLB
  state,                          // "loaded" | "failed"
  items,                          // [T]; empty when failed
  error_code,                     // typed error; null unless failed
  error                           // displayable text; null unless failed
}

PublicationParentIssue {          // GHP-FR-FTMC
  number, title, issue_type, url,
  milestone                       // { number, title } | null
}
PublicationIssueType { name }
PublicationMilestone { number, title }

PublicationMetadata {             // GHP-FR-MDLD
  repository_owner, repository_name,
  settings,                       // GithubPublicationSettings
  parents,                        // MetadataList<PublicationParentIssue>
  issue_types,                    // MetadataList<PublicationIssueType>
  milestones,                     // MetadataList<PublicationMilestone>
  sub_issue_type                  // { name, resolved }: resolved is the Type's GitHub
                                  // spelling, or null when unavailable (GHP-FR-ETJD)
}

PublicationChoice {               // saved in the attempt and the record (GHP-FR-ATCH)
  kind,                           // "root" | "sub_issue"
  parent_repository_owner,        // string | null; null for a root issue
  parent_repository_name,         // string | null
  parent_issue_number,            // number | null
  issue_type,                     // string | null
  milestone_policy,               // MilestonePolicy | null; null for a root issue
  milestone_number,               // number | null
  milestone_title                 // string | null
}
```

### Tauri commands
- `"get draft publication (draft id)"` → `get_draft_publication(draft_id)` → `DraftPublicationView` (GHP-FR-CWTG).
- `"list publication remotes (draft id)"` → `list_publication_remotes(draft_id)` → `PublicationRemoteResolution` (GHP-FR-WKDE).
- `"publish draft to github (draft id, remote name, persist remote, publication choice)"` → `publish_draft_to_github(draft_id, remote_name, persist_remote, publication_choice)` → `PublicationOutcome` (GHP-FR-CVYK).
- `"load publication metadata (draft id, remote name)"` → `load_publication_metadata(draft_id, remote_name)` → `PublicationMetadata` (GHP-FR-MDLD).
- `"get github publication settings"` → `get_github_publication_settings()` → `GithubPublicationSettings` (GHP-FR-KVRH).
- `"set github publication settings (parent issue types, sub issue type, sub issue milestone policy)"` → `set_github_publication_settings(parent_issue_types, sub_issue_type, sub_issue_milestone_policy)` → `GithubPublicationSettings` (GHP-FR-NQWX).
- `"list github issue types"` → `list_github_issue_types()` → `MetadataList<PublicationIssueType>` (GHP-FR-PTYL).
- `"retry draft publication (draft id)"` → `retry_draft_publication(draft_id)` → `PublicationOutcome` (GHP-FR-OWLB).
- `"resolve draft publication conflict (draft id, choice)"` → `resolve_draft_publication_conflict(draft_id, choice)` → `PublicationOutcome`, `choice` being `"update_existing"` or `"publish_new"` (GHP-FR-YPGL).
- `"cancel draft publication conflict (draft id)"` → `cancel_draft_publication_conflict(draft_id)` (GHP-FR-EBSA).
- `"cancel draft publication attempt (draft id)"` → `cancel_draft_publication_attempt(draft_id)` (GHP-FR-NAXT).
- `"open publication issue (draft id, url)"` → `open_publication_issue(draft_id, url)` (GHP-FR-MJTB).

Every command errors with `no_project_open` when no project is open and `draft_not_found` when `draft_id` names no draft in the active worktree.

### Events
- `"draft publication changed"` — emitted whenever a draft's publication store changes. Payload: `{ draft_id }`. Consumed by `../ui/NAW-new-artifact.md` and `../ui/DFI-draft-information.md`.

### Internal (Rust API, not registered as Tauri commands)
- `resolve_publication_repository()` → `{ remote_name, repository_owner, repository_name }` or a GHP-FR-ZRFP refusal (GHP-FR-YDAN). `GPP-github-polling.md` (GPP-FR-RGNM) is the only caller.

### Consumed from other modules
- `resolve_github_token_secret()` — the project's token (`GTS-github-token-storage.md` GTS-FR-13).
- `read_prompt_images(draft_id)` — the saved prompt's images (`DAS-draft-assets.md` DAS-FR-22).
- `read_draft_prompt(id)`, `draft_record(id)`, `draft_publication_path(id)`, and `set_draft_published(id)` — `DRS-draft-storage.md` (DRS-FR-38, DRS-FR-EJBM, DRS-FR-VKQO).
- `load_publication_remote_selection()` and `save_publication_remote_selection(selection)` — `PSS-project-settings-storage.md` (PSS-FR-TQFB).
- `load_github_publication_settings()` and `save_github_publication_settings(settings)` — `PSS-project-settings-storage.md` (PSS-FR-HWBG).

## Non-functional requirements
- Enumeration and validation are read-only against GitHub: opening the publish flow costs at most one repository read per GitHub remote and creates nothing, so an author who opens the flow and changes their mind has changed nothing anywhere.
- One publication is at most one search, one create or one edit, one sub-issue link, and one atomic store write, whatever the size of the prompt.
- Reading the three metadata lists costs a bounded number of requests: one Type read, at most three pages of one hundred rows for the milestones, and at most three such pages for each configured parent Type.
- Every network call carries the project's own token and no other credential, and a call that cannot be authenticated is refused before it is made.
- The publication store is small and is read whole; a draft with a long history opens as fast as one with none.
