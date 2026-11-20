# Git submodules

**Spec code:** `GSM`

## Intent
Backend rules that let the author use a git submodule as a folder of the project. A project can hold submodules, and the author edits, reviews, commits and pushes the files inside them in the same way as all other files. This spec defines the **repository set**: the main repository and the submodules that it holds. It also defines how a project-relative path finds its repository, and how one operation of `GTC-git.md` or `CHC-changes.md` acts over more than one repository. Out of scope: the initialisation, update, addition and removal of a submodule; a nested repository that no `.gitmodules` file lists; and graduation runs and work streams inside a submodule.

## Functional requirements
1. **GSM-FR-VPAS** The **repository set** of the active worktree holds the main repository (per `GTC-git.md` GTC-FR-02) and every initialised submodule at any depth whose root is inside the project content root. A submodule of a submodule is a member when its own parent is a member.
2. **GSM-FR-SFEI** A member is identified by its project-relative root path. The main repository has the path `""`. Paths use `/` as the separator on every platform.
3. **GSM-FR-FRLZ** A submodule that `.gitmodules` lists but that has no checkout is not a member. A submodule that the module cannot open is not a member, and the module logs one `WARN` record that names its path and the cause.
4. **GSM-FR-SIWR** The **owner** of a project-relative path is the member whose root is the nearest folder above the path (per `ASC-artifact-scanning.md` ASC-FR-LXOQ). The root path of a submodule is owned by its parent member, where it is the submodule pointer.
5. **GSM-FR-MUTH** Each operation that takes a project-relative path acts on the owner of that path, and gives the path to Git relative to the root of the owner. The operations that report a path report it project-relative.
6. **GSM-FR-FCOF** A **pointer change** is a submodule whose checked-out `HEAD` commit is not the commit that its parent records for it at the comparison base. A change to a file inside the submodule makes no pointer change.
7. **GSM-FR-KWWZ** When a commit names paths in more than one member, the module makes one commit in each of these members, with the same message. It commits the deepest member first and the main repository last.
8. **GSM-FR-MPWK** After the module makes a commit in a submodule, the commit of the parent member records the new pointer of that submodule. This occurs also when the caller named no path of the parent.
   - *Why:* A submodule commit that the parent does not record is lost for every other clone of the project.
9. **GSM-FR-ZKEJ** When the commit in one member fails, the module makes no more commits. The commits that it made before the failure stay. The error names the members that it committed, their commit ids, and the member that failed.
10. **GSM-FR-PQZX** A rollback of a submodule pointer path writes nothing for that path and reports it as the typed failure `submodule_pointer`.
11. **GSM-FR-REIT** A push of the current branch pushes each **affected** member: a member whose current branch holds commits that its upstream does not hold, or that has a remote and no upstream. It pushes the deepest member first and the main repository last.
12. **GSM-FR-MJHC** The push skips a member that has a detached `HEAD` with the reason `detached_head`, and a member that has no remote with the reason `no_remote`. A skip does not stop the push of the other members.
13. **GSM-FR-GTPI** When the push of one member fails, the module pushes no more members. The members that it pushed before the failure stay pushed.
   - *Why:* A parent pushed after a failed submodule push records a pointer that the remote of the submodule does not hold.
14. **GSM-FR-GHFY** The main branch write check (per `GTC-git.md` GTC-FR-JWOL) runs on the branch of the main repository only. A commit or a push in a submodule does not run it.
15. **GSM-FR-VFJO** The module computes the repository set again for each operation. A submodule that is initialised or removed while the project is open joins or leaves the set at the next operation.
16. **GSM-FR-WPKJ** The module logs each commit and each push that it makes in a member, and each member that it skips, with the member path.

## Contract surface
This spec adds no Tauri command. `CHC-changes.md` and `GTC-git.md` compose these internal Rust primitives:

- `repository_set(content_root)` → `[Member]`, the main repository first and the submodules after it in path order (GSM-FR-VPAS).
- `owner_of(set, path)` → the member that owns `path` (GSM-FR-SIWR).

```
Member {
  path,                      // project-relative root; "" for the main repository
  name,                      // last segment of path; the content root folder name for the main repository
  parent?,                   // path of the parent member; absent for the main repository
  branch?,                   // current branch; absent when HEAD is detached
  head_id?                   // full commit id at HEAD; absent when the branch has no commit
}
```

`RepositoryInfo` in `CHC-changes.md` is the wire form of `Member`.

## Non-functional requirements
- A project with no submodule has a repository set of one member, and every operation behaves as it does for one repository.
- The repository set is read from the Git configuration and the `.git` entries on disk. It makes no network request.
