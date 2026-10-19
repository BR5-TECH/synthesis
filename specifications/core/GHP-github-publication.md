# GitHub publication

**Spec code:** `GHP`

## Intent
The backend that publishes a draft's prompt as a **GitHub issue**, so that work prepared in the New Artifact tab can be picked up by an agent that runs outside this application. It resolves which configured Git remote the issue goes to, checks that the repository accepts new issues and that the project's GitHub token can read and write them, writes an **attempt record** before the first GitHub request, creates the issue through the REST API of the host of the project's token (`github.com` or a GitHub Enterprise host), and appends one record to the draft's **append-only publication history**. A draft that holds a current record can be **updated**: the issue of that record takes the draft's current name and prompt as its title and body, and one more record is appended. The backend keeps the parent, Type, milestone, and label lists of the publication remote in an in-memory **metadata cache**, which it refreshes in the background when the project opens, every 30 minutes, and on each change of what the lists depend on, so the publication chooser and the note publish modal open with lists that they do not wait for. The issue is published as a **root issue** or as a **sub-issue** of an open issue in the same repository, with an optional GitHub issue **Type** and **milestone**; the author's choices are saved in the attempt record before the first GitHub mutation. Publication is a fact about a draft's metadata rather than about its prompt: it never edits the prompt, never removes a draft, never changes the draft's status, and never changes local graduation. The backend also publishes a **note** as an issue, with a title and a description that the author writes, a parent or none, a Type, a milestone, and labels, and then deletes the note, so a note that has outgrown the project **moves** to GitHub; a note publication keeps no attempt record, no marker, and no history. Out of scope: the remote agent that reads the issue and works on it; labels for a draft publication, and assignees, comments, and issue state for any publication, which neither a publication nor an update changes; media attachment and local-asset upload, which v1 refuses rather than performs; adding an issue to a GitHub Project, the publication having no Project selector of its own; minting or storing a token, which is `GTS-github-token-storage.md`'s; and the storage layout of the publication store, which is `DRS-draft-storage.md`'s.

## Functional requirements
1. **GHP-FR-QJNV** Publication targets the host of the remote and reaches it through a programmatic HTTPS client against the REST API base that `GTS-github-token-storage.md` GTS-FR-PDWB resolves from that host, authenticated with the secret the open project resolves through `GTS-github-token-storage.md` (GTS-FR-13). The `gh` CLI is neither required nor invoked, and no operation here shells out to any external program.
2. **GHP-FR-WKDE** `list_publication_remotes(draft_id)` enumerates **every configured Git remote of the project** and **mutates nothing on GitHub**. It classifies each remote, resolves which one the next attempt would use, and reports the origin of that choice. It creates no issue, edits no issue, and writes no attempt record.
3. **GHP-FR-BXTU** A remote is `kind = "github"` when its URL canonicalizes to a repository on a GitHub host, in either the HTTPS or the SSH form, and `kind = "other"` otherwise. The SSH form is the `ssh://` URL or the scp-like address `[user@]host:owner/repo`, with any user name or none: `git@github.com:acme/widgets.git` and `company@company.ghe.com:acme/widgets.git` are both GitHub remotes. A GitHub host is `github.com`, a host that ends in `.ghe.com`, or the host of a stored token. Canonicalization drops the scheme, the credentials, the port, a trailing `.git`, and a trailing slash, and lowercases the host, so one repository reached by two URL forms canonicalizes to one value.
4. **GHP-FR-MZPR** A GitHub remote is **eligible** only when three facts hold: the repository record is readable, the repository accepts new issues, and the project-resolved token may write issues to it. A remote that fails one carries the eligibility value naming which — `issues_inaccessible`, `issues_disabled`, `issues_create_forbidden`, `token_unavailable`, or `host_mismatch` — together with the exact reason as displayable text. A remote is `host_mismatch` when the project token belongs to another host than the remote, and no request is made for it.
5. **GHP-FR-TKBW** A repository **accepts new issues** only when its repository record reports Issues turned on, and reports the repository neither archived nor disabled. A repository that does not carries `issues_disabled`, whose displayable reason names the repository setting rather than the token.
6. **GHP-FR-RQDV** The token **may write issues** unless the granted scopes it reports exclude it: a classic token must report `repo`, or `public_repo` for a public repository. A token that reports no scopes at all is fine-grained and is not inferred against. Code-write permission on the repository is never required.
    - *Why:* Anybody who can read a repository with Issues on may open an issue in it, so a push-permission test refuses every repository the author does not own.
7. **GHP-FR-LTAC** A remote whose `kind` is `"other"` is never eligible and carries `not_github` as its reason. It is reported rather than omitted, so the author reads why a remote they configured cannot receive an issue.
8. **GHP-FR-HVQG** A **persisted choice** is used only when a remote of that name is still enumerated, its canonicalized URL still matches the persisted one, and that remote is eligible. Otherwise it is ignored for this attempt and is not rewritten, so a remote rename or URL change invalidates it.
9. **GHP-FR-XAUP** Where no persisted choice applies and **exactly one** configured remote exists and is eligible, that remote is used automatically. Where **two or more** configured remotes exist, the caller chooses one from the whole enumeration.
10. **GHP-FR-NDSB** `origin` states how the reported `selection` was reached: `automatic` for the single eligible remote of GHP-FR-XAUP, `persisted` for a stored choice that still applies, `attempt_only` for a remote the caller named without asking to persist it, and `none` where no remote resolves.
11. **GHP-FR-ZRFP** Where the project has no configured remote, no GitHub remote, or no eligible remote, `selection` is null, `origin` is `none`, and publication is refused with the typed error naming which case holds: `no_remote_configured`, `no_github_remote`, `github_host_mismatch`, `token_unavailable`, `issues_inaccessible`, `issues_create_forbidden`, or `issues_disabled`.
12. **GHP-FR-WNJC** Where more than one refusal holds across the project's GitHub remotes, the refusal names the first of `github_host_mismatch`, `token_unavailable`, `issues_inaccessible`, `issues_create_forbidden`, `issues_disabled` that holds.
    - *Why:* The first three are conditions of the token and stop every remote; `issues_disabled` is a condition of one repository, and naming it first sends the author to the wrong settings page.
13. **GHP-FR-CVYK** `publish_draft_to_github(draft_id, remote_name, persist_remote, publication_choice)` re-resolves the named remote before it publishes: the remote must still be enumerated and still eligible at the moment of the call. One that is not is refused with its own typed error and publishes nothing. A persisted choice is validated the same way on every attempt.
14. **GHP-FR-PWXA** `persist_remote` true writes the chosen remote's name and canonicalized URL into project-local settings (per `PSS-project-settings-storage.md` PSS-FR-TQFB) and applies to every later attempt in this project. `persist_remote` false writes nothing and applies the choice to the current attempt alone.
15. **GHP-FR-GJEO** Only a draft whose status is `active`, the legacy `published` (per `DRS-draft-storage.md` DRS-FR-VKQO), or `graduated` may be published. An `archived` draft is refused with `draft_archived`, whose displayable reason states that the draft must be restored before publication.
16. **GHP-FR-BKLT** A GitHub-shadow draft (per `DRS-draft-storage.md` DRS-FR-WFLY) is refused by every publication operation with `draft_github_shadow`, and nothing is written or requested. `get_draft_publication` reports it as not publishable with that reason.
16. **GHP-FR-SRDL** A draft a non-terminal graduation run holds is refused with `draft_locked_by_graduation` (per `DRS-draft-storage.md` DRS-FR-19), and nothing is written or requested.
17. **GHP-FR-AKUM** Before the first GitHub request of an attempt, publication reads the **saved prompt's images** through `DAS-draft-assets.md`'s `read_prompt_images` (DAS-FR-22) and refuses with `local_assets_unsupported` where any image reference is not an absolute `http` or `https` URL. The error carries every affected reference. No issue and no remote asset is created.
    - *Why:* v1 uploads nothing, so an issue carrying a relative path would render a broken image to whoever picks the work up.
18. **GHP-FR-DTVW** An image reference that is already an absolute `http` or `https` URL is carried into the issue body unchanged and blocks nothing.
19. **GHP-FR-FQIZ** An attempt's **marker** is an opaque value generated once, unique across the project, and stable for the whole life of that attempt. It is written into the issue body on its own line as `<!-- synthesis-publication-marker: <opaque-marker> -->` and is searchable in an issue body by exact match. It is never written into the draft's prompt.
20. **GHP-FR-XOBH** The issue **title** is exactly the draft's current name. The issue **body** is the complete current prompt followed by the marker line of GHP-FR-FQIZ and nothing else.
21. **GHP-FR-RUYT** Before the first GitHub request, the attempt record is written **atomically** to the draft's publication store (per `DRS-draft-storage.md` DRS-FR-EJBM). It holds the draft id, the marker, the remote, the repository, the state `open`, and the publication choice of GHP-FR-ATCH. The store stays in the checkout that wrote it and no commit names it (per `DRS-draft-storage.md` DRS-FR-WBTA).
22. **GHP-FR-KZAP** A draft holds **at most one non-terminal attempt**. `publish_draft_to_github` against a draft that already holds one is refused with `attempt_in_progress` and starts nothing; the standing attempt is continued through `retry_draft_publication` or ended through `cancel_draft_publication_attempt`.
23. **GHP-FR-VNCS** A publication that fails at any step **writes no draft status** (GHP-FR-BSYH), appends no history record, and leaves the attempt record standing in a non-terminal state, so the attempt stays recoverable. The typed error naming the exact failure is returned.
24. **GHP-FR-OWLB** `retry_draft_publication(draft_id)` reuses the standing attempt's marker, remote, repository, and publication choice, and **searches the target repository's issues for that exact marker before creating anything**. It always sends the draft's **latest** name as the title and its latest prompt as the body.
    - *Why:* An issue creation whose response was lost still created the issue, so a retry that did not search would post the same work twice.
25. **GHP-FR-IEQC** Where the search finds no issue carrying the marker, the retry creates the issue on the terms of GHP-FR-XOBH. Where the search cannot answer — GitHub is unreachable, or the search is refused — the retry returns a typed error and creates nothing.
26. **GHP-FR-TMBK** Where the search finds an issue carrying the marker whose title and body equal the latest title and body and whose parent, Type, and milestone match the saved choice (GHP-FR-CMPR), the retry **reuses that issue** and creates no second one, then completes the attempt on the terms of GHP-FR-JAWD.
27. **GHP-FR-HRUN** Where the search finds an issue carrying the marker whose title, body, parent, Type, or milestone differs from the latest title and body and the saved choice, the retry returns `recovery_required` carrying that issue's number and URL and the list of what differs, and moves the attempt to `awaiting_choice`. It edits and creates nothing on its own.
28. **GHP-FR-YPGL** `resolve_draft_publication_conflict(draft_id, "update_existing")` reconciles the found issue to the latest title, body, and saved choice (GHP-FR-RCNL), keeps the attempt's marker, and completes the attempt with **one** history record. `"publish_new"` ends the attempt without a history record and starts a **deliberate re-publication**: a new attempt, a new marker, the saved choice, and a new issue.
29. **GHP-FR-EBSA** Cancelling the recovery choice — `cancel_draft_publication_conflict(draft_id)` — returns the attempt to `open`, appends no history record, and edits nothing on GitHub, so the attempt stays recoverable.
30. **GHP-FR-JAWD** A successful publication appends **one** record carrying the provider `github`, the repository owner and name, the issue number, the issue URL, the publication instant in RFC 3339 UTC, the attempt's marker, and its publication choice (GHP-FR-HSCH), and clears the attempt, in **one atomic write** of the store. An update appends its record on the terms of GHP-FR-WUJQ.
31. **GHP-FR-UZMX** The history is **append-only**. No operation here updates, reorders, replaces, or deletes a record already in it. A publication and an update each append one record. The **current publication** is the newest record; every earlier record stays readable and keeps naming the issue it named.
32. **GHP-FR-QLDF** A **deliberate re-publication** of a draft that already has history always starts a new attempt with a new marker and creates a new issue. It never reuses a marker from a completed record and never edits an issue that a record names. Only the update of GHP-FR-MZMW edits such an issue, and only the issue of the current record.
33. **GHP-FR-BSYH** A successful publication **writes no draft status**. It appends its history record to the draft's publication store and changes nothing in the draft's record: an `active` draft stays `active`, a `graduated` draft stays `graduated`, and a legacy `published` draft stays `published` (per `DRS-draft-storage.md` DRS-FR-VKQO).
34. **GHP-FR-NAXT** `cancel_draft_publication_attempt(draft_id)` abandons the standing attempt: the attempt record is cleared, no history record is appended, and nothing on GitHub is created, edited, or deleted. An issue an abandoned attempt already created stays on GitHub and is not recorded.
35. **GHP-FR-CWTG** `get_draft_publication(draft_id)` returns the draft's current record, its whole history newest-first, its standing attempt or null, and its current eligibility. It is a **read**: it contacts GitHub only for the eligibility checks of GHP-FR-MZPR and writes nothing.
36. **GHP-FR-ZFPI** On application restart every standing attempt is read back from the publication store and stays recoverable. A draft holding one is never left permanently unpublishable: an `open` attempt offers retry and an `awaiting_choice` attempt offers the recovery choice of GHP-FR-YPGL, and neither blocks abandoning it.
37. **GHP-FR-MJTB** `open_publication_issue(draft_id, url)` opens the OS default browser at that URL and opens no page inside the application. It refuses a URL that neither a record of that draft's history nor the draft's GitHub-shadow issue link (per `DRS-draft-storage.md` DRS-FR-XDWS) holds, and one that is not an HTTPS issue address on the repository host that the record or the link names, so this operation cannot open an arbitrary address.
38. **GHP-FR-DHXK** No error payload, metadata view, stored record, log line, or event of this module carries a token secret or a remote URL with an embedded credential, on the terms `GTC-git.md` GTC-FR-11 sets for Git output.
39. **GHP-FR-YDAN** `resolve_publication_repository()` resolves the project's publication remote on the terms of GHP-FR-HVQG and GHP-FR-XAUP without a draft, and returns the remote name and the repository host, owner, and name, or the refusal code of GHP-FR-ZRFP. It writes nothing and creates nothing on GitHub.
39. **GHP-FR-PVOA** The typed error vocabulary is exactly `no_project_open`, `draft_not_found`, `draft_archived`, `draft_locked_by_graduation`, `local_assets_unsupported`, `no_remote_configured`, `no_github_remote`, `issues_inaccessible`, `issues_disabled`, `issues_create_forbidden`, `token_unavailable`, `github_host_mismatch`, `attempt_in_progress`, `no_attempt`, `github_unreachable`, `issue_create_failed`, `issue_update_failed`, `no_publication_record`, `issue_not_found`, `issue_moved`, `issue_update_forbidden`, `publication_store_write_failed`, `draft_github_shadow`, `parent_issues_unreadable`, `issue_types_unreadable`, `milestones_unreadable`, `parent_issue_unavailable`, `issue_type_unavailable`, `milestone_unavailable`, `invalid_publication_choice`, `sub_issue_link_failed`, `invalid_publication_settings`, and `github_project_not_selected`, `labels_unreadable`, `label_unavailable`, `note_not_found`, `invalid_note_publication`, and `issue_url_refused`. Each carries displayable text stating the exact failure.

40. **GHP-FR-KVRH** The **GitHub publication settings** are five project-public values stored through `PSS-project-settings-storage.md` (PSS-FR-HWBG): the **parent issue Types** (default `Feature`), the **sub-issue Type** (default `Task`), the **sub-issue milestone policy** (default `inherit_parent`), the **root issue Type** (default No Type), and the **note issue Type** (default `Feature`). `get_github_publication_settings()` returns them with defaults applied to every unset value.
41. **GHP-FR-NQWX** `set_github_publication_settings(parent_issue_types, sub_issue_type, sub_issue_milestone_policy, root_issue_type, note_issue_type)` persists the five values and returns them. A null `root_issue_type` or `note_issue_type` stores No Type. It refuses an empty Type list, an empty Type name, and an unknown policy with `invalid_publication_settings`, and writes nothing. It never checks a Type against GitHub. A failed write returns `publication_store_write_failed`.
42. **GHP-FR-PTYL** `list_github_issue_types()` resolves the publication repository on the terms of GHP-FR-YDAN, reads the issue Types of that repository's owner, and returns the Type list of GHP-FR-DZLB. A repository that does not resolve returns its GHP-FR-ZRFP refusal. It mutates nothing on GitHub.
43. **GHP-FR-MDLD** `load_publication_metadata(draft_id, remote_name)` re-resolves the named remote on the terms of GHP-FR-CVYK and returns the repository, the settings of GHP-FR-KVRH, the **parent list**, the **Type list**, the **milestone list**, the **label list**, the resolved sub-issue Type, the resolved root issue Type, and the resolved note issue Type. It mutates nothing on GitHub and writes no attempt record.
44. **GHP-FR-DZLB** Each of the four lists is `{ state, items, error_code, error }` with `state` either `loaded` or `failed`. One list that fails to read does not fail the command and does not change the other lists. A `loaded` list may hold no item.
45. **GHP-FR-FTMC** A **parent issue** is an open issue of the selected publication repository that is not a pull request and whose Type equals a configured parent issue Type, compared without regard to case. A row carries the number, title, Type, milestone or null, and URL. The request names each Type in the owner's spelling where the owner lists it.
46. **GHP-FR-YSPJ** The **Type list** holds the names of the issue Types that GitHub reports for the repository's owner. An owner that GitHub reports no Types for, including a user account, gives a `loaded` list with no item. Any other failed read gives `failed` with `issue_types_unreadable`.
47. **GHP-FR-GHEA** The **milestone list** holds the open milestones of the selected repository, each with its number and title. A failed read gives `failed` with `milestones_unreadable`. A failed parent read gives `failed` with `parent_issues_unreadable`.
47. **GHP-FR-XLXU** The **label list** holds the labels of the selected repository, each with its name, its colour, and its description or null. A repository with no label gives a `loaded` list with no item. A failed read gives `failed` with `labels_unreadable`. Draft publication reads the list and sends no label.
48. **GHP-FR-UXOT** No failed metadata read refuses root publication. A root issue publishes without a Type when the Type list holds none or failed, and without a milestone when the milestone list holds none or failed.
49. **GHP-FR-BWNI** The **publication choice** passed to `publish_draft_to_github` is `{ parent_issue_number, issue_type, milestone_number }`, each value a number, a string, or null. A null `parent_issue_number` requests a **root issue**. A number requests a **sub-issue** of that issue in the selected publication repository.
50. **GHP-FR-LCKZ** For a root issue, a named `issue_type` must equal a Type of the Type list and a named `milestone_number` must be an open milestone of the milestone list. Otherwise the call is refused with `issue_type_unavailable` or `milestone_unavailable`, and nothing is written or requested to create an issue.
51. **GHP-FR-RMVQ** For a sub-issue, the call reads the parent and refuses with `parent_issue_unavailable` unless the parent exists, is open, is not a pull request, and has a configured parent issue Type. A sub-issue choice that names an `issue_type` is refused with `invalid_publication_choice`. Nothing is written for a refused call.
    - *Why:* The sub-issue Type is set in Project settings alone, so the choice never carries one.
52. **GHP-FR-AFEX** The resolved **root issue Type** is null where no root issue Type is stored. Otherwise it is the stored name and its spelling in the Type list, or a null spelling where the Type list does not hold it or failed. A root issue Type that does not resolve sends no Type.
52. **GHP-FR-JGXW** The resolved **note issue Type** is null where No Type is stored as the note issue Type. Otherwise it is the stored name and its spelling in the Type list, or a null spelling where the Type list does not hold it or failed.
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
64. **GHP-FR-HSTA** Every request of publication for one repository uses the REST API base of the repository host: the eligibility read, the marker search, issue creation and update, the issue read of an update, the parent issue read, the Type list, the milestone list, the label list, the note publication create, and the sub-issue link and unlink. The token is sent only to that API base.
65. **GHP-FR-HSTB** Each `DraftPublicationRecord` and each `DraftPublicationAttempt` carries the `repository_host` of its repository. A record or an attempt stored without it reads as `github.com`. The issue address a record stores is the address that GitHub returned for the issue.
66. **GHP-FR-HSTC** Where the host of a repository does not support issue Types or sub-issues, the Type list, the parent issue read, or the sub-issue link reports its existing typed unavailable state (GHP-FR-YSPJ, GHP-FR-ETJD, `sub_issue_link_failed`). A root publication with no Type stays available, and the author is asked for no extra input.
67. **GHP-FR-MZMW** `update_draft_on_github(draft_id, update_closed)` edits the issue that the draft's **current record** names, and no other issue. A draft that holds no record is refused with `no_publication_record`, and nothing is requested.
    - *Why:* An earlier record names an issue that the author replaced on purpose, so an update of it would undo that choice.
68. **GHP-FR-LVVD** The update applies the refusals of a publication before its first request: `draft_github_shadow`, `draft_locked_by_graduation`, `draft_archived`, `attempt_in_progress`, and `local_assets_unsupported`. It writes no attempt record and starts no attempt.
69. **GHP-FR-ZRPL** The update targets the repository host, owner, and name and the issue number of the current record. It resolves the project token on the terms of GHP-FR-QJNV. It refuses with `token_unavailable` where no token resolves, and with `github_host_mismatch` where the token belongs to another host than the record.
70. **GHP-FR-SVXP** Before it writes, the update reads the issue. A deleted or missing issue gives `issue_not_found`. An issue that GitHub reports under another repository or number gives `issue_moved`. A read that GitHub refuses for the token gives `issue_update_forbidden`, and a read that cannot answer gives `github_unreachable`. Each writes nothing.
71. **GHP-FR-AXRU** Where the issue is closed and `update_closed` is false, the update answers `issue_closed` with the issue number and URL, and writes nothing on GitHub or on disk. Where `update_closed` is true, the update edits the closed issue. The update never opens or closes an issue.
72. **GHP-FR-TCFV** The edit is one partial request that names the title and the body alone. The title is the draft's current name. The body is the current prompt followed by the marker line of the current record (GHP-FR-FQIZ). Milestone, Type, parent, labels, assignees, and state stay as GitHub holds them. A refused edit gives `issue_update_failed`.
73. **GHP-FR-WUJQ** A successful edit appends one record in one atomic write. It copies the host, owner, name, issue number, issue URL, publication instant, marker, and publication choice of the current record, and adds the **updated** instant in RFC 3339 UTC. The update answers `updated` with that record and emits `"draft publication changed"`.
74. **GHP-FR-BCAV** The update changes no draft status. A failed update appends no record and writes nothing to disk. An edit that GitHub accepted, followed by a store write that fails, gives `publication_store_write_failed`.
75. **GHP-FR-IQTK** The **metadata cache** holds the parent list, the Type list, the milestone list, and the label list of GHP-FR-DZLB for one resolved publication remote, with the remote name and the repository host, owner, and name. Each list keeps its own state. The cache is in memory only and is shared by every window of the application.
76. **GHP-FR-ROBR** A project close, a project switch, and a change of the active worktree empty the cache. A refresh result that belongs to a project or a worktree that is no longer active is discarded and changes nothing, on the terms of `PST-project-storage.md` PST-FR-37.
77. **GHP-FR-HGVL** The **run condition** holds only when the GitHub polling settings select a GitHub Project (per `PSS-project-settings-storage.md` PSS-FR-OPQD) and the publication remote resolves on the terms of GHP-FR-YDAN, which requires an eligible project token. Where no GitHub Project is selected, no refresh runs, the cache stays empty, and nothing is raised or logged as a failure.
78. **GHP-FR-AWEZ** A **refresh** resolves the publication remote, reads the four lists through the read of GHP-FR-MDLD, and merges them into the cache. It runs off the window thread and blocks no command. At most one refresh runs at a time per open project. A request made while one runs starts no second refresh and receives the outcome of the running one.
79. **GHP-FR-EYGR** A list that the refresh reads as `loaded` replaces the cached list. A list that fails to read keeps the cached list unchanged, and is stored as `failed` only where the cache holds no list for that repository. A refresh for another repository than the cached one replaces the whole cache.
80. **GHP-FR-MSLY** A refresh **fails** where the run condition does not hold, where one list fails to read, or where it does not end within 120 seconds. A refresh that times out or does not resolve the remote leaves every cached list as it was. Every failure is logged with its typed error code.
81. **GHP-FR-JIAU** Where the merged lists differ from the cached lists in a state, an item, or the order of the items, the refresh replaces the cache and emits `"publication metadata changed"` once with the new cache. A refresh that changes nothing emits nothing. Emptying a cache that holds lists emits the event with no cache.
82. **GHP-FR-ISJI** When a project opens with a GitHub Project selected, the backend starts one **startup refresh**. Where it fails, including where the remote or the token does not resolve, the backend emits `"publication metadata startup failed"` once with the typed error code and its displayable text, and keeps that failure in memory until it is taken (GHP-FR-KPXF). A project open, a project switch, a change of the active worktree, and a project close discard a kept failure. Where no GitHub Project is selected, no startup refresh runs, nothing is emitted, and nothing is kept.
83. **GHP-FR-RVCL** While a project is open, the backend starts one **periodic refresh** 30 minutes after the start of the previous refresh, as a best effort. A tick that falls due while a refresh runs, or while the run condition does not hold, starts none. A failed or timed out periodic refresh is logged and is never shown to the author.
84. **GHP-FR-JDMU** A change of the selected GitHub Project, the persisted publication remote selection, the project token (`"github tokens changed"`), the parent issue Types, or the active worktree starts one refresh at once and restarts the 30-minute timer. Its failure is logged and never shown to the author. A change that leaves no GitHub Project selected empties the cache.
85. **GHP-FR-JTYL** `get_publication_metadata_cache(remote_name)` returns the cached lists as a `PublicationMetadata`, with the settings and the resolved Types of GHP-FR-MDLD computed from the current settings, where the cache holds lists for a remote of that name. Otherwise it returns null. It contacts no network and writes nothing.
86. **GHP-FR-SIMK** `refresh_publication_metadata()` starts a refresh, or joins the running one, and answers when it ends with its outcome: `changed`, `unchanged`, or `failed` with the typed error and its displayable text, together with the cache as it then stands. Where no GitHub Project is selected, it answers `failed` with `github_project_not_selected` and reads nothing.
87. **GHP-FR-KPXF** `take_publication_metadata_startup_failure()` returns the startup failure that GHP-FR-ISJI keeps, as `{ error_code, error }`, and discards it, so each kept failure is returned at most once. Where no failure is kept, or no project is open, it returns null. It contacts no network and writes nothing to disk. The event can come before a window listens for it, and this command lets that window still receive the failure.
88. **GHP-FR-CNPX** `get_note_publication_availability()` answers whether a note can be published now. It is `available` only where the run condition of GHP-FR-HGVL holds, with the remote name and the repository host, owner, and name. Otherwise it carries `github_project_not_selected` or the GHP-FR-ZRFP refusal, with its displayable text. It writes nothing and creates nothing on GitHub.
89. **GHP-FR-JGNI** `publish_note_to_github(note_id, title, body, choice)` publishes a note as an issue of the publication repository and then deletes the note. It refuses with `github_project_not_selected` or the GHP-FR-ZRFP refusal where the run condition does not hold, and with `note_not_found` where the id names no note of the active worktree.
90. **GHP-FR-HKMC** The call refuses with `invalid_note_publication` where the trimmed title is empty, where the title holds more than 256 characters, or where the body holds more than 65,536 characters, each counted in Unicode code points. The issue title is the trimmed title, and the issue body is the body exactly as sent, with no marker line.
91. **GHP-FR-VLDP** Before it creates anything, the call reads what the choice names: a named Type must be a Type of the owner, a named milestone an open milestone, and each named label a label of the repository. Otherwise it refuses with `issue_type_unavailable`, `milestone_unavailable`, or `label_unavailable`. A refused call creates no issue and deletes no note.
92. **GHP-FR-WXJD** A choice with a parent requests a sub-issue. The call reads the parent and refuses with `parent_issue_unavailable` on the terms of GHP-FR-RMVQ. The sub-issue takes the Type and the milestone that the choice names: the sub-issue Type and the sub-issue milestone policy of GHP-FR-KVRH do not apply to a note.
93. **GHP-FR-WJTX** The call sends one create request that carries the title, the body, and, where the choice names them, the Type, the milestone number, and the label names. For a sub-issue it then sends one sub-issue link request, on the terms of GHP-FR-OHGY.
94. **GHP-FR-GCLP** A note publication writes no attempt record, no marker, no publication store, and no history record, and keeps no retry or recovery state. It emits no `"draft publication changed"`. A second call for the same note is a new publication.
95. **GHP-FR-BYZQ** A create request that GitHub refuses returns `issue_create_failed`, and one that cannot reach GitHub returns `github_unreachable`. In each case no issue exists, the note stays, and nothing is written.
96. **GHP-FR-QWFD** A sub-issue link that fails answers `sub_issue_link_failed` with the number and the URL of the created issue. The issue stays on GitHub without its parent, and the note stays.
97. **GHP-FR-WJBM** After the issue is created and linked, the call deletes the note through the transaction of `NTC-notes-storage.md` NTC-FR-21, which also removes its reminder and its discussion. Where the deletion succeeds, the call answers `published` with the issue number and the issue URL that GitHub returned.
98. **GHP-FR-GIVY** Where the deletion fails, the call answers `note_not_removed` with the issue number, the issue URL, and the typed error code of the deletion. The issue stays on GitHub, and the note stays as the failed step of NTC-FR-21 leaves it.
99. **GHP-FR-MAMB** `open_publication_repository_issue(url)` opens the OS default browser at that URL and needs no draft. It resolves the publication repository on the terms of GHP-FR-YDAN, and refuses with `issue_url_refused` a URL that is not an HTTPS issue address of that repository on its host. It opens no page inside the application.
100. **GHP-FR-MAMT** No log line, error payload, or event of note publication carries the title, the body, or any text of the note. A log line names the note id, the repository, and the issue number alone.

## Contract surface
Every operation is a Tauri command invoked by `../ui/NAW-new-artifact.md`, `../ui/NIP-note-issue-publication.md`, `../ui/NTS-notes.md`, or `../ui/SET-project-settings.md`. The quoted name is the canonical operation name and matches the "Delegated to backend (abstract)" line of the spec that invokes it byte-for-byte.

### Record shapes
The persisted shapes — `DraftPublicationRecord`, `DraftPublicationAttempt`, and the store holding them — are `DRS-draft-storage.md`'s (DRS-FR-EJBM) and are not restated here. Both carry the `PublicationChoice` below as `choice`.

```
PublicationRemoteEligibility =
  | "eligible" | "not_github" | "issues_inaccessible"
  | "issues_disabled" | "issues_create_forbidden" | "token_unavailable" | "host_mismatch"

PublicationRemote {
  name,                 // the configured Git remote's name
  url,                  // canonicalized remote URL, host included (GHP-FR-BXTU)
  kind,                 // "github" | "other"
  repository_host,      // null when kind is "other"
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

IssueUpdateOutcome =              // GHP-FR-AXRU, GHP-FR-WUJQ
  | { kind: "updated", record }
  | { kind: "issue_closed", issue_number, issue_url }

GithubPublicationSettings {       // GHP-FR-KVRH
  parent_issue_types,             // [string], at least one
  sub_issue_type,                 // string
  sub_issue_milestone_policy,     // MilestonePolicy
  root_issue_type,                // string | null; null is No Type
  note_issue_type                 // string | null; null is No Type
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
PublicationLabel { name, color, description }   // GHP-FR-XLXU; description: string | null

PublicationMetadata {             // GHP-FR-MDLD
  repository_owner, repository_name,
  settings,                       // GithubPublicationSettings
  parents,                        // MetadataList<PublicationParentIssue>
  issue_types,                    // MetadataList<PublicationIssueType>
  milestones,                     // MetadataList<PublicationMilestone>
  labels,                         // MetadataList<PublicationLabel>
  sub_issue_type,                 // { name, resolved }: resolved is the Type's GitHub
                                  // spelling, or null when unavailable (GHP-FR-ETJD)
  root_issue_type,                // { name, resolved } | null; null when none is
                                  // stored (GHP-FR-AFEX)
  note_issue_type                 // { name, resolved } | null; null when No Type is
                                  // stored (GHP-FR-JGXW)
}

PublicationMetadataRefresh {      // GHP-FR-SIMK
  outcome,                        // "changed" | "unchanged" | "failed"
  error_code,                     // typed error; null unless failed
  error,                          // displayable text; null unless failed
  remote_name,                    // the cached remote's name, or null
  metadata                        // PublicationMetadata of the cache, or null
}

PublicationMetadataCacheChanged { // GHP-FR-JIAU
  remote_name,                    // the cached remote's name, or null when emptied
  metadata                        // PublicationMetadata of the cache, or null when emptied
}

NotePublicationAvailability {     // GHP-FR-CNPX
  available,                      // bool
  reason_code,                    // typed error; null when available
  reason,                         // displayable text; null when available
  remote_name,                    // null unless available
  repository_host, repository_owner, repository_name   // null unless available
}

NotePublicationChoiceInput {      // sent by the caller (GHP-FR-JGNI)
  parent_issue_number,            // number | null; null requests a root issue
  issue_type,                     // string | null; null is No Type
  milestone_number,               // number | null
  labels                          // [string]; may be empty
}

NotePublicationOutcome =          // GHP-FR-QWFD, GHP-FR-WJBM, GHP-FR-GIVY
  | { kind: "published", issue_number, issue_url }
  | { kind: "sub_issue_link_failed", issue_number, issue_url }
  | { kind: "note_not_removed", issue_number, issue_url, error_code }

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
- `"set github publication settings (parent issue types, sub issue type, sub issue milestone policy, root issue type, note issue type)"` → `set_github_publication_settings(parent_issue_types, sub_issue_type, sub_issue_milestone_policy, root_issue_type, note_issue_type)` → `GithubPublicationSettings` (GHP-FR-NQWX).
- `"get publication metadata cache (remote name)"` → `get_publication_metadata_cache(remote_name)` → `PublicationMetadata | null` (GHP-FR-JTYL).
- `"refresh publication metadata"` → `refresh_publication_metadata()` → `PublicationMetadataRefresh` (GHP-FR-SIMK).
- `"take publication metadata startup failure"` → `take_publication_metadata_startup_failure()` → `{ error_code, error } | null` (GHP-FR-KPXF). The main window invokes it for the raise of `../ui/NTF-notifications.md` NTF-FR-POEW.
- `"list github issue types"` → `list_github_issue_types()` → `MetadataList<PublicationIssueType>` (GHP-FR-PTYL).
- `"retry draft publication (draft id)"` → `retry_draft_publication(draft_id)` → `PublicationOutcome` (GHP-FR-OWLB).
- `"resolve draft publication conflict (draft id, choice)"` → `resolve_draft_publication_conflict(draft_id, choice)` → `PublicationOutcome`, `choice` being `"update_existing"` or `"publish_new"` (GHP-FR-YPGL).
- `"cancel draft publication conflict (draft id)"` → `cancel_draft_publication_conflict(draft_id)` (GHP-FR-EBSA).
- `"cancel draft publication attempt (draft id)"` → `cancel_draft_publication_attempt(draft_id)` (GHP-FR-NAXT).
- `"update draft on github (draft id, update closed)"` → `update_draft_on_github(draft_id, update_closed)` → `IssueUpdateOutcome` (GHP-FR-MZMW).
- `"open publication issue (draft id, url)"` → `open_publication_issue(draft_id, url)` (GHP-FR-MJTB).
- `"get note publication availability"` → `get_note_publication_availability()` → `NotePublicationAvailability` (GHP-FR-CNPX). Invoked by `../ui/NTS-notes.md`.
- `"publish note to github (note id, title, body, note publication choice)"` → `publish_note_to_github(note_id, title, body, choice)` → `NotePublicationOutcome` (GHP-FR-JGNI). Invoked by `../ui/NIP-note-issue-publication.md`.
- `"open publication repository issue (url)"` → `open_publication_repository_issue(url)` (GHP-FR-MAMB). Invoked by `../ui/NIP-note-issue-publication.md`.

Every command except `take_publication_metadata_startup_failure` errors with `no_project_open` when no project is open. Every command that takes a `draft_id` errors with `draft_not_found` when it names no draft in the active worktree.

### Events
- `"draft publication changed"` — emitted whenever a draft's publication store changes. Payload: `{ draft_id }`. Consumed by `../ui/NAW-new-artifact.md`, `../ui/DFI-draft-information.md`, and `../ui/DRP-drafts-panel.md`.
- `"publication metadata changed"` — emitted when a refresh changes the metadata cache or the cache is emptied (GHP-FR-JIAU). Payload: `PublicationMetadataCacheChanged`. Consumed by `../ui/NAW-new-artifact.md` and `../ui/NIP-note-issue-publication.md`.
- `"publication metadata startup failed"` — emitted once when the startup refresh fails (GHP-FR-ISJI). Payload: `{ error_code, error }`. Consumed by the main window, which then takes the kept failure (GHP-FR-KPXF) and raises the toast of `../ui/NTF-notifications.md` NTF-FR-POEW.

### Internal (Rust API, not registered as Tauri commands)
- `resolve_publication_repository()` → `{ remote_name, repository_host, repository_owner, repository_name }` or a GHP-FR-ZRFP refusal (GHP-FR-YDAN). `GPP-github-polling.md` (GPP-FR-RGNM) the refresh of the metadata cache (GHP-FR-AWEZ), and the note publication operations (GHP-FR-CNPX, GHP-FR-JGNI, GHP-FR-MAMB) are its callers.

### Consumed from other modules
- `resolve_github_token_secret()` — the project's token (`GTS-github-token-storage.md` GTS-FR-13).
- `read_prompt_images(draft_id)` — the saved prompt's images (`DAS-draft-assets.md` DAS-FR-22).
- `read_draft_prompt(id)`, `draft_record(id)`, and `draft_publication_path(id)` — `DRS-draft-storage.md` (DRS-FR-38, DRS-FR-EJBM).
- `load_publication_remote_selection()` and `save_publication_remote_selection(selection)` — `PSS-project-settings-storage.md` (PSS-FR-TQFB).
- `load_github_publication_settings()` and `save_github_publication_settings(settings)` — `PSS-project-settings-storage.md` (PSS-FR-HWBG).
- `load_github_polling_settings()` — the selected GitHub Project of the run condition, `PSS-project-settings-storage.md` (PSS-FR-OPQD).
- `"github tokens changed"` — the token change that starts a refresh, `GTS-github-token-storage.md` (GTS-FR-AEQO).
- `note_record(id)` and the deletion transaction of `delete_note(id)` — the note a note publication reads and removes, `NTC-notes-storage.md` (NTC-FR-25, NTC-FR-21, NTC-FR-VPJZ).

## Non-functional requirements
- Enumeration and validation are read-only against GitHub: opening the publish flow costs at most one repository read per GitHub remote and creates nothing, so an author who opens the flow and changes their mind has changed nothing anywhere.
- One publication is at most one search, one create or one edit, one sub-issue link, and one atomic store write, whatever the size of the prompt. One update is one issue read, one edit, and one atomic store write.
- Reading the four metadata lists costs a bounded number of requests: one Type read, at most three pages of one hundred rows for the milestones and for the labels, and at most three such pages for each configured parent Type.
- One note publication is the reads that validate its choice, one create, at most one sub-issue link, and one note deletion.
- Every network call carries the project's own token and no other credential, and a call that cannot be authenticated is refused before it is made.
- The publication store is small and is read whole; a draft with a long history opens as fast as one with none.
- One refresh of the metadata cache costs the requests of one metadata read, and at most one refresh runs at a time, so the 30-minute schedule adds a small, bounded load on GitHub.
