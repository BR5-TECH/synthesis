## Intent

IDE should be able to work with both regular githbub and github enterprise deployments.

The user gives only a host and a token. The backend resolves everything else, including the API base URL. Enterprise support covers every operation that uses a token: validate, push/pull, pull request, publication, and polling.

## User journey

- User opens general settings.
- User opens GitHub tab
- User clicks Add token
- Overlay window shows up.
- User is able to specify the domain for github, with a placeholder being set to [github.com](http://github.com)
- The dialog has an **Open GitHub token page** button. It opens the token-creation page on the typed domain (github.com when the field is empty).
- User adds the token. The domain is saved on the token record and cannot be changed after add. To use another host, the user adds a new token.
- Token rows and the project token picker show the domain only when it is not github.com.
- If a project remote is on a host that differs from the host of the project's token, the operation fails with a typed error. The UI shows it inline.

## Requirements

- Empty domain field must be treated as github.com, without requiring a user to provide default value explicitly.
- [github.com](http://github.com) stays default integration. enterprise domains like \*.ghe.com is an option.
- Existing tokens users have configured should work “as is” since the [github.com](http://github.com) is a default.
- The domain is normalized before save: trim, lowercase, remove scheme, path, and trailing slash. Reject any value that is not a valid host with an inline error in the dialog. Do not store the value as typed.
- Any host is accepted. The backend resolves the API base URL from the host with deterministic rules: github.com uses api.github.com, `*.ghe.com` uses api.<sub>.ghe.com, any other host uses https://<host>/api/v3. No other input is needed from the user.
- Each token record has a host. A record saved before this change has no host and is read as github.com. No migration step and no user action is needed.
- A token is used only for remotes on its own host. For any other remote, the backend returns a typed host-mismatch error and makes no network request. The project token binding stays per project.
- The "open github token creation page" operation takes the token host. It opens the token page of that host with the scopes the application needs pre-selected.
- Validate, push, pull, fetch, pull request, publication, and polling accept a remote on the token host, not only github.com. Remote and repository resolution must parse enterprise hosts.
- GitHub Projects, issue publication, and polling work on an enterprise host. Example: a project whose remote is on `company.ghe.com` and whose token has that host. Listing GitHub Projects, reading Project items and the `Status` field, listing issue Types, milestones and parent issues, creating issues and sub-issues, and polling for ready tasks all use the API URLs resolved from the token host.
- The backend resolves the GraphQL URL from the host with deterministic rules: github.com uses `https://api.github.com/graphql`, `*.ghe.com` uses `https://api.<sub>.ghe.com/graphql`, any other host uses `https://<host>/api/graphql`.
- Issue and pull request URLs that the application stores, shows, or opens in the browser use the token host, not github.com. The publication record, the shadow draft, and the pending claim keep the host with the repository.
- If an enterprise host does not support a GitHub feature (for example issue Types, sub-issues, or Projects), the feature reports its existing typed "unavailable" state. Root publication stays available. The user is not asked for any extra input.

## Specifications to update

- `specifications/ui/GHA-github-authentication.md`: domain field, normalization error, token page button, domain on rows and picker, host-mismatch error.
- `specifications/core/GTS-github-token-storage.md`: host on the token record, default github.com, API URL resolution, host-matched token resolution, token creation page by host.
- `specifications/core/GSS-global-settings-storage.md`: host field in the GitHub token registry.
- `specifications/core/GTC-git.md`: GTC-FR-09, GTC-FR-14 and related rules that now say github.com only.
- `specifications/core/GHP-github-publication.md`: remove "hosts other than `github.com`" from out of scope; use the resolved API URLs for issue, Type, milestone, and sub-issue requests; keep the host in publication records.
- `specifications/ui/GIT-git.md`: Ready tasks rows and issue links use the token host.
- `specifications/ui/SET-project-settings.md`: the GitHub Project settings section (Project picker, Type and milestone controls) works for the enterprise host of the project token.
- `specifications/core/GPP-github-polling.md`: GPP-FR-WOIM and related rules say `github.com` only; use the resolved GraphQL URL for Project listing, Project reads, and polling.
- `specifications/ui/SET-project-settings.md`: project token display shows the domain when it is not github.com (SET-FR-12).