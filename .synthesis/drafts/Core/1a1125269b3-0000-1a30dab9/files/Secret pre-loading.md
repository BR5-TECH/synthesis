## Intent

The application currently reads the operating-system keyring for managed secrets as operations need them. Repeated keyring reads can add delay or repeat access prompts. This feature keeps one process-local cache of all application-managed secrets. It initializes the cache lazily, when the first secret-dependent operation runs, and reuses the cached values until the application process exits. It does not load secrets merely because the application starts.

## User journey

- The user starts the application. Startup does not read secrets from the keyring.
- The first operation that needs a managed secret, including a secret-presence check, initializes the process cache from the consolidated vault entry. The vault completes its existing legacy migration before the cache can be used.
- The operation that triggers initialization uses the cache if initialization succeeds. Later operations reuse it and do not read the consolidated entry again just to fetch or check a secret.
- If initialization fails, the requesting operation receives the existing vault error. The cache stays uninitialized, and that operation is not retried automatically. The next independent secret-dependent operation tries initialization again.
- A secret write or removal still uses the vault's verified-write behavior. The cache changes only after the keyring write and read-back verification succeed. On failure, callers receive the existing error and the cache remains unchanged.
- When the application process exits, its cache is discarded. The cache is never written to the filesystem.

## Requirements

- Cache every application-managed secret in the consolidated application secret vault, not only the secret requested first. Include secret-presence checks in the cache behavior.
- Initialize lazily on the first secret-dependent vault operation in each process. Startup alone must not read the keyring. A successful initialization reads the consolidated vault entry once; failed initialization leaves the cache uninitialized and may be retried by the next independent secret-dependent request.
- Allow at most one initialization attempt at a time. Concurrent requests during initialization share its result; they must not cause parallel bulk reads. If initialization fails, return that failure to the waiting requests and let a later request retry.
- Complete the existing legacy-secret migration before serving the cache, so migrated values are included. Preserve the existing migration safety and verified-write rules.
- After initialization, serve secret reads and presence checks from the process cache without another keyring read. Do not interpret the existing “exactly once” wording as one keyring access total: verified writes, read-back checks, and migration may still access the keyring.
- Keep existing verified keyring writes and removals. Update the in-memory cache only after the corresponding keyring write and read-back verification succeed. A failed mutation must not change the cache or be reported as successful.
- If cache initialization fails, return the vault's existing typed error to the operation that requested it. Do not retry that same operation automatically. Retry initialization only on the next independent secret-dependent request.
- Assume one application process uses the keyring at a time. Detecting or reconciling changes made by another process is out of scope; state this limit so a stale cache is not presented as cross-process safe.
- Keep secrets and secret-derived text out of logs, errors, events, IPC payloads, and filesystem storage. Keep secret-bearing values out of debug output. The cache exists in process memory only and is discarded when that process exits.
- Preserve current caller behavior and error mapping. A cache failure must reach the requesting operation through its existing vault error path; do not add frontend commands or events for the cache.
- Add backend tests for lazy initialization, reuse of cached reads and presence checks, concurrent initialization, retry after a failed load, cache updates only after verified mutations, and unchanged cache state after failed mutations.

## Specifications to update

- Update `specifications/core/ASV-application-secret-vault.md` to own the process cache, its initialization and synchronization behavior, its error and retry behavior, its relationship to legacy migration and verified mutations, and its process lifetime. Replace the current no-retention rule without weakening the existing secret-safety, lock, migration, or write-verification requirements.
- Update `specifications/core/GTS-github-token-storage.md`, `specifications/core/AAP-ai-api-integrations.md`, and `specifications/core/AIC-agentic-integrations.md` to remove their conflicting rules that prohibit the vault from retaining secrets between operations. State that the vault owns the cache; preserve each module's limits on when and to whom a secret is returned, and all existing logging, IPC, and persistence protections.
- Review other caller contracts, including `specifications/core/GTC-git.md` and `specifications/tools/EAC-execute-agent-cli.md`. Change them only where their wording directly conflicts with the vault-owned cache. A caller must not gain permission to retain a resolved secret beyond its own operation or launch.