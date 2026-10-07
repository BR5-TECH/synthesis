# Application secret vault

**Spec code:** `ASV`

## Intent
The one place in the application that reads and writes a secret in the operating system keyring. It keeps every application-managed secret — each GitHub token, each AI API provider key, each agent credential, and every secret type added later — inside a single keyring entry that holds one extensible object. One entry is what keeps an operation to one keyring access however many secrets are stored, so the author answers one authentication prompt for an operation instead of one prompt for each secret. One process cache of the decoded object, filled when the first secret-dependent operation runs, goes further: after that operation the process reads the entry for no secret and no presence check. That single entry is also the whole of what this module writes: it recovers from a value it cannot decode, and it takes over the secrets of the earlier one-entry-per-secret layout, without ever putting application data anywhere but there. The modules that own secrets — `GTS-github-token-storage.md`, `AAP-ai-api-integrations.md`, and `AIC-agentic-integrations.md` — address their secrets here by path and hold no keyring entry of their own. Out of scope: what a secret means, which one applies, and when to read one, all of which belong to the owning module; the descriptive facts about a secret, which live in the user-global settings store (`GSS-global-settings-storage.md`); and credential storage a vendor owns, such as the login directory the Codex CLI writes for itself, which this module neither reads nor moves.

## Contract surface
The module owns one keyring entry, the object inside it, the process cache of that object, the process-wide lock that guards both, and the migration of earlier per-secret entries into it. It registers no Tauri command and is unreachable from the frontend; every operation below is internal and is called only by a module that owns secrets.

### The stored object
The entry holds one JSON object, `AppSecrets`, serialized as one UTF-8 string:

```
AppSecrets {
  version:    1,
  github:     { tokens:    { <token_id>:    <secret> } },
  ai_api:     { providers: { <provider_id>: <secret> } },
  agentic:    { vendors:   { <vendor_id>:   <secret> } },
  projects:   { <project_key>: { <namespace>: { <key>: <secret> } } },
  quarantine: string | null,   // a prior value of this entry that would not decode (ASV-FR-17)
}
```

### The secret path
A secret is addressed by a `SecretPath`: the ordered list of object keys from the root of `AppSecrets` to the leaf that holds the secret string.

```
SecretPath = string[]

github token id T    ->  ["github",  "tokens",    T]
ai api provider P    ->  ["ai_api",  "providers", P]
agentic vendor V     ->  ["agentic", "vendors",   V]
project-scoped value ->  ["projects", <project_key>, <namespace>, <key>]
```

### The process cache
The cache holds the decoded `AppSecrets` object and the serialized value the entry held when the cache last agreed with it. It is empty when the process starts and is filled by the first secret-dependent operation (ASV-FR-ZUGZ). It lives in process memory only. It has no operation of its own, so no caller addresses it: every operation below reads and writes it on the caller's behalf.

```
cache state  = uninitialized | initialized { object, serialized }
```

### Operations (internal; no Tauri command and no UI consumer)

```
read_secret(path: SecretPath)            -> Option<String> | VaultError
secret_presence(paths: SecretPath[])     -> { <path>: bool } | VaultError
apply_secret_mutations(m: Mutation[])    -> () | VaultError
migrate_legacy_secrets(c: Candidate[])   -> MigrationOutcome | VaultError

Mutation  = Set { path: SecretPath, secret: string }
          | Remove { path: SecretPath }

Candidate = { legacy_service: string, legacy_account: string, path: SecretPath }

MigrationOutcome {
  adopted:    number,   // legacy secrets written into the consolidated object
  deleted:    number,   // legacy entries removed after the verified write
  undeleted:  number,   // legacy entries the keyring refused to delete
  unreadable: number,   // candidates whose legacy entry the keyring refused to read
  absent:     number,   // candidates that have no legacy entry at all
}

VaultError = vault_unavailable
           | vault_write_failed
           | vault_verify_failed
           | vault_malformed
           | vault_unsupported_version
```

`Mutation`, `Candidate`, and the value `read_secret` returns are Rust types that implement neither `Serialize` nor `Deserialize`, and whose `Debug` rendering contains no secret material. `MigrationOutcome` carries counts and no path, no id, and no secret.

## Functional requirements
1. **ASV-FR-01** Every application-managed secret is held in one operating-system keyring entry, reached through the `keyring` crate. The entry has the service name `com.synthesis.secrets` and the account name `application-secrets`. Both names are stable, and the application writes to no other keyring entry — not for a secret, not for a copy, and not for a value it could not read.
2. **ASV-FR-02** The value of the entry is one `AppSecrets` object, serialized as one UTF-8 JSON string. The object carries a `version` field whose value is `1`.
3. **ASV-FR-03** A leaf of `AppSecrets` is a secret string. Each level between the root and a leaf is an object keyed by an identifier the owning module chooses: a token id under `github.tokens`, a provider id under `ai_api.providers`, a vendor id under `agentic.vendors`, and a project key under `projects`.
4. **ASV-FR-04** A secret is addressed by its `SecretPath` and by nothing else. This module gives a path no meaning: it does not know what a token id is, and it never reads a path to decide what to do.
5. **ASV-FR-05** A new secret type needs a new path, not a new schema. This module accepts a path of any depth, creates each absent object between the root and the leaf when it writes, and refuses no path because the schema of this version does not name it.
6. **ASV-FR-06** A read-modify-write keeps every field it does not know. A namespace, an object, or a leaf that this version does not name is written back unchanged, so a build that adds a secret type does not destroy the secrets of a build that adds a different one.
7. **ASV-FR-07** An absent namespace is empty rather than wrong. Reading a path whose namespace, intermediate object, or leaf is absent reports that the path holds no secret, and it is not an error. Writing to such a path creates every level it needs.
8. **ASV-FR-08** The `projects` namespace holds a secret value only where that value belongs to one project. A project binding, an integration override, a model selection, and every other non-secret fact about a project stay in the user-global settings store (`GSS-global-settings-storage.md` GSS-FR-23, GSS-FR-26, GSS-FR-28), and no module writes one into this entry.
9. **ASV-FR-09** One process-wide lock guards the entry and the process cache. Every read, every mutation, and every migration holds the lock for its whole sequence, so two sequences never touch the entry or the cache at the same time.
10. **ASV-FR-10** A mutation is a modify-write of the whole object. The module applies every mutation of the request to a copy of the cached object, serializes the copy, and writes the complete object to the entry as one keyring value. It never writes a fragment of the object.
11. **ASV-FR-11** A write is complete only after verification. The module reads the entry back inside the same held lock and compares the decoded result with the object it wrote. A mutation is successful only where the comparison succeeds. The read-back is the one keyring read that the cache does not replace.
12. **ASV-FR-12** A failed mutation changes nothing. Where the write, the read-back, or the comparison fails, the entry holds the value it held before the request, the cache holds the object it held before the request, the module returns the typed error of ASV-FR-31, and none of the requested mutations is applied.
13. **ASV-FR-13** A request carries a list of mutations and applies them together. A `Set` and a `Remove` in one request reach the keyring in one write, so the entry never holds a state in which some of them are applied and others are not.
14. **ASV-FR-14** Concurrent requests are serialized by the lock, and each one takes the cache after it takes the lock. A request therefore modifies the object the request before it wrote, and no update is lost, whatever order the requests arrive in.
15. **ASV-FR-15** A keyring that refuses a read returns `vault_unavailable`. The module reports no path as empty for that reason, so a caller can always tell a keyring that did not answer from a path that holds nothing.
16. **ASV-FR-16** An entry that does not exist reads as an empty `AppSecrets` object of the current version. This is the state of a machine that has stored no secret, and it is neither `vault_unavailable` nor `vault_malformed`: every path reports no secret, every presence answer is false, and the first mutation creates the entry.
17. **ASV-FR-17** A stored value that is not a well-formed `AppSecrets` object puts the entry in the **quarantined** state: every read and every presence request returns `vault_malformed`, and the module writes nothing and deletes nothing until a mutation asks it to. The next mutation builds an empty `AppSecrets` object, applies that request's mutations to it, and carries the undecodable prior value verbatim in the object's `quarantine` field. That one object — the new secrets and the quarantined text together — is written to the same entry as the single verified write of ASV-FR-10 and ASV-FR-11, so recovery destroys nothing and needs no second keyring entry. A write or a verification that fails leaves the undecodable value where it is, on the terms of ASV-FR-12.
18. **ASV-FR-18** `quarantine` is a reserved root field rather than a namespace. No `SecretPath` addresses it, no operation returns it, and nothing decodes it or reads a secret out of it. It is treated as secret material throughout, so ASV-FR-28 governs it exactly as it governs a leaf. A later read-modify-write carries it unchanged (ASV-FR-06), and the entry holds one such field: a value quarantined a second time is carried whole, whatever it held, rather than accumulating a chain of fields.
19. **ASV-FR-19** A stored object whose `version` is higher than the version this build knows returns `vault_unsupported_version` from every read and every mutation. Nothing is written and nothing is deleted, so an older build never overwrites an object a newer build wrote, and it never quarantines one either.
20. **ASV-FR-20** `migrate_legacy_secrets` moves the secrets of the earlier one-entry-per-secret layout into the consolidated entry. It runs once per process, as the first step of the cache initialization of ASV-FR-ZRWU, so the cache serves no request before migration has completed. Its candidates are supplied by the owning modules, because a keyring enumerates no entry: `GTS-github-token-storage.md` supplies one candidate per registry record at service `com.synthesis.github-token`, `AAP-ai-api-integrations.md` one per configured provider at service `com.synthesis.ai-api-provider`, and `AIC-agentic-integrations.md` one per vendor that holds a credential at service `com.synthesis.agentic-integration`, each with the secret's id as the legacy account name.
21. **ASV-FR-21** Migration adopts a legacy secret only into a path the consolidated object does not already hold. Where both hold a value for one path, the consolidated object wins and the legacy value is adopted nowhere.
22. **ASV-FR-22** A candidate whose legacy entry the keyring refuses to read is skipped rather than fatal. It is counted in `unreadable`, its secret is adopted nowhere, and its legacy entry is **not deleted**, because deleting a secret this module could not read would destroy the only copy of it. Migration goes on to the remaining candidates and returns its outcome, so one unreadable legacy entry never blocks the adoption of the others, and the same candidate is offered again at the next migration.
23. **ASV-FR-23** A candidate that has no legacy entry at all adopts nothing, deletes nothing, and is counted in `absent`. It is not an error and not a failure of migration, because it is the normal state of every candidate once migration has completed and of every candidate on a machine that never used the earlier layout.
24. **ASV-FR-24** Migration performs one whole-object write and no more, whatever the number of secrets it adopts. It performs that write whenever it has a secret to adopt or a legacy entry to delete — including where it adopts nothing at all, because a deletion may follow only a verified write (ASV-FR-25). A migration with nothing to adopt and nothing to delete writes nothing.
25. **ASV-FR-25** Migration deletes no legacy entry until it has written the consolidated object and verified the read-back (ASV-FR-11). The rule holds whatever that write had to change: a candidate whose path the consolidated object already held (ASV-FR-21) is deleted after the same verified write as every other, and never ahead of it. No operation anywhere in the application deletes a legacy entry before the consolidated object that holds its secret has been written and verified.
26. **ASV-FR-26** Partial migration is safe and repeatable. A legacy entry the keyring refused to delete stays where it is and is counted in `undeleted`; the next migration writes and verifies the object again and deletes it then, and adopting it a second time changes nothing because the consolidated object already holds its path (ASV-FR-21). A migration that fails before the verified write leaves every legacy entry and the consolidated entry exactly as they were.
27. **ASV-FR-27** The process cache is the only place where this module keeps a secret between requests. The module holds no other copy of a secret, of the decoded object, or of the serialized value after a request ends, and no operation returns a reference to the cache.
28. **ASV-FR-28** No secret, and no text derived from a secret, is written to `app_data_dir()/synthesis.toml`, to a file under a project's `.synthesis/`, to a log line, to an error payload, to an event payload, to a URL, or to a `Debug` rendering. A log line about a vault operation names the operation and its outcome, and never a path, an id, a quarantined value, or a secret.
29. **ASV-FR-29** This module registers no Tauri command, so no frontend call reaches it. No operation returns the whole object, a whole namespace, the `quarantine` field, or a list of secret values; `read_secret` returns one secret to one caller that asked for one path.
30. **ASV-FR-30** `secret_presence(paths)` answers presence for every supplied path from the process cache, and returns booleans rather than values. A listing that describes many records therefore makes no keyring access once the cache is initialized, whatever the number of records.
31. **ASV-FR-31** The typed errors are `vault_unavailable`, `vault_write_failed`, `vault_verify_failed`, `vault_malformed`, and `vault_unsupported_version`. Each owning module renders every one of them across the IPC boundary as its existing `keychain_unavailable` error (`GTS-github-token-storage.md` GTS-FR-14, `AAP-ai-api-integrations.md` AAP-FR-20, `AIC-agentic-integrations.md` AIC-FR-24), so the error vocabulary the frontend sees does not depend on how secrets are stored.
32. **ASV-FR-32** Credential storage a vendor owns is outside this module. The Codex CLI's own login directory is written, read, and owned by that CLI (`AIC-agentic-integrations.md` AIC-FR-20), and this module neither reads it, nor copies it, nor migrates it into the entry.
33. **ASV-FR-33** Every operation in the contract surface exists with a typed payload in the walking-skeleton build. A stub may hold the object in memory and skip the keyring, provided it keeps the lock, the whole-object write, the read-back verification, and the typed errors, and returns no secret beyond the one path a caller asked for.

34. **ASV-FR-ZUGZ** The process cache is empty when the process starts, and starting the application reads no keyring entry. The first secret-dependent operation of the process initializes the cache. Those operations are `read_secret`, `secret_presence`, `apply_secret_mutations`, and a `migrate_legacy_secrets` call that has at least one candidate. A call with no candidate reads nothing.
35. **ASV-FR-ZRWU** Initialization first completes the legacy migration (ASV-FR-20). It then holds the decoded object of the consolidated entry, with every secret that migration adopted. It reads the consolidated entry once. An absent entry initializes the cache to an empty object (ASV-FR-16). The cache serves no request before initialization completes.
36. **ASV-FR-DYCW** The cache holds every secret of the consolidated object, not only the secret that the first request named. Every path of every namespace, including a namespace this build does not name (ASV-FR-06), is served from it.
37. **ASV-FR-CIDB** At most one initialization attempt runs at a time. A request that arrives while an attempt runs waits for it, starts no keyring read of its own, and takes the result of that attempt: the initialized cache, or the typed error of ASV-FR-31.
38. **ASV-FR-FTFU** An initialization that fails leaves the cache uninitialized. The operation that requested it, and each request that waited for it, receives the typed error of ASV-FR-31 through its existing error path. The module does not retry for that operation. The next independent secret-dependent request starts a new attempt.
39. **ASV-FR-DQHY** After initialization, `read_secret` and `secret_presence` answer from the cache and make no keyring access. A secret that a verified mutation set or removed is the answer the next request receives.
40. **ASV-FR-GLJD** The cache takes the new object only after the write and the read-back verification of ASV-FR-11 succeed, inside the same held lock. A mutation that fails at any step, or that is refused because a path cannot be created, leaves the cache as it was. Where the rollback of a failed verification also fails, the entry has unknown content, and the module empties the cache so the next request initializes it again. The module reports a failed mutation as a failure, never as a success.
41. **ASV-FR-DIEF** Where initialization fails with `vault_malformed` (ASV-FR-17), the next mutation reads the entry, quarantines the undecodable value, and writes one verified object. The cache then holds that verified object, so recovery initializes the cache. A read or presence request still returns `vault_malformed` until a mutation succeeds.
42. **ASV-FR-SUXZ** The cache exists in process memory only and ends when the process ends. It is written to no file, no log line, no event, and no IPC payload. Its `Debug` rendering shows no secret, no path, and no id (ASV-FR-28).
43. **ASV-FR-ELQN** The module assumes that one application process uses the consolidated entry at a time. It does not detect or reconcile a change that another process makes to the entry after initialization. The cache is not safe across processes, and the module does not claim it is.

## Non-functional requirements
- The operation that initializes the cache makes at most one keyring read, one keyring write, and one read-back. After initialization, a read or a presence request makes no keyring access, and a mutation makes one keyring write and one read-back. The count does not grow with the number of stored secrets, which is what lets one authentication cover an operation where the operating system keyring permits it.
- Migration is the one sequence that reads more than one keyring entry. It reads each candidate legacy entry once, on the one launch that finds any, and never again once the entries are deleted; a candidate it could not read is read again at the next migration and nowhere else.
- The serialized object stays small — a few kilobytes for a store holding tens of secrets — which is within the value-size limit of the credential store on every supported platform. A quarantined value is bounded by what the entry already held, so recovery cannot take the entry past that limit by more than the one value it retains.
- The lock is held for the keyring access alone. No network request, no filesystem access, and no user interaction runs while it is held, so one slow operation cannot block every other secret operation for an unbounded time.
- The entry is visible in the operating system's own credential manager. Its service and account names disclose the application and nothing about what it holds, and no second entry appears there under any condition.
