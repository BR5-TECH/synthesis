# UI verification with Playwright

This is how you drive the synthesis frontend in a real browser to check that a feature actually
works. It exists because the Vitest suite proves the *logic* — it renders components in jsdom
against a mocked `invoke` and asserts on the resulting tree — while jsdom sees none of what a
browser actually does: layout, stacking, focus, scrolling, real event dispatch, CSS.

A feature can pass every unit test and still be unreachable, clipped, or unusable in the window.
The question this verification answers is: **in a real rendering engine, can a user reach and use
what the spec describes?**

## The harness

`harness/` in this skill serves the real frontend with every `@tauri-apps/api/*` import aliased to
an in-memory stand-in, so the app boots in an ordinary browser with no Tauri process behind it.
This is what makes the UI Playwright-drivable — the platform WebView the shipped app renders in is
not.

From the repo root:

```
pnpm exec vite --config .claude/skills/engineer/harness/vite.config.ts
```

It serves on **http://localhost:5199** — deliberately not 1420, so it can run alongside the
developer's own `pnpm tauri dev` without fighting for the port.

```
harness/
├── vite.config.ts   # root = repo, aliases the four @tauri-apps modules, port 5199
├── mock-core.ts     # invoke() — a switch over every command, plus seeded demo data
├── mock-event.ts    # listen/once, plus window.__fireBusEvent to fire backend events
├── mock-window.ts   # getCurrentWindow, monitors, sizing
└── mock-dialog.ts   # file/folder pickers, message/ask/confirm
```

`mock-core.ts` seeds a demo project called **acme** — a library tree, comment threads, notes,
changes, history. Getting into the shell is just: navigate to the URL, click the `acme` row in the
recent-projects list. From there the whole app is live.

To simulate a backend-emitted event, call the hook `mock-event.ts` installs:

```js
window.__fireBusEvent("tree-changed", { /* payload */ })
```

## Keeping the harness current

The mocks are a parallel implementation of the backend contract, so they drift as `src/api.ts`
grows. When your feature calls a command `mock-core.ts` doesn't handle, the app will break in a way
that looks like a bug but isn't — so **extend the mock as part of the verification**, don't work
around it. Add the `case` to the switch in `invoke`, return a shape that matches `src/types.ts`, and
seed data consistent with the existing `acme` fixtures so the rest of the app stays coherent.

Two shortcuts worth taking:

- **Crib payload shapes from the tests.** The `*.test.tsx` file next to the component you changed
  already has realistic mock responses for these commands. Copy them rather than reconstructing
  shapes from `src/types.ts` by hand.
- **Read the console first.** An unhandled command usually surfaces as an undefined-property error
  in `browser_console_messages`, which names the command faster than reading code does.

Leave the extended mock in place when you're done — it's a shared asset, and the next verification
starts further along because of it.

## Always headless

**Drive the browser headless unless the user explicitly asks to watch it.** A verification run is
machine-checked measurement, not a demo: a window stealing focus interrupts whoever is at the
keyboard, and nothing about a visible window makes the findings better. Screenshots come back the
same either way.

Headless is a property of **how the Playwright server was launched**, not a per-call option — an
agent already connected to a headed server cannot switch mid-run, and telling it to "run headless"
achieves nothing. So check before you spawn:

```
for f in ~/.claude/plugins/cache/claude-plugins-official/playwright/*/.mcp.json; do echo "$f"; cat "$f"; done
```

**Check every one of those files individually.** There are several — one per plugin revision, plus
an `unknown/` — and they do *not* agree: the hash-named ones have carried `--headless` while
`unknown/` shipped without it. A bare `cat` over the glob concatenates them into output that reads
as uniform at a glance, so the one bad file is invisible and a window opens anyway. Loop, or diff
them; do not eyeball the concatenation.

The args must include `--headless`:

```json
{ "playwright": { "command": "npx", "args": ["@playwright/mcp@latest", "--headless"] } }
```

Editing that file takes effect on the **next** session; the running server keeps its old args. When
you need headless *now* and the server is headed, skip the MCP server and drive Playwright directly
from a script — no restart, no window, and the measurements are the same:

```js
import { chromium } from "<npx-cache>/node_modules/playwright/index.mjs";
const browser = await chromium.launch({ headless: true, executablePath: "<chrome-headless-shell>" });
```

Find the two paths with `ls ~/.npm/_npx/*/node_modules/playwright` and
`find ~/Library/Caches/ms-playwright -name chrome-headless-shell`. The cached npx package often
expects a build revision one off from the installed browser, which is why `executablePath` is worth
naming explicitly rather than letting it resolve.

A scripted run has a second advantage worth reaching for on a *re-*verification: assertions with
real numbers. `getBoundingClientRect()` on the two elements plus an overlap predicate turns "the
control covers the page" into a pass/fail with coordinates, and it re-runs in seconds after a fix.

## What to look at (and what to leave alone)

Spend the effort where jsdom is blind:

- **Reachability.** Can you get to the feature by the route the spec describes — the menu item, the
  shortcut, the button? A control that renders but sits behind another element, or off the visible
  area, passes unit tests and fails users.
- **Layout and overflow.** Does the surface fit? Does content clip, overlap, or push the shell
  around? Resize (`browser_resize`) — panels and overlays are where this breaks. Measuring with
  `browser_evaluate` over `getBoundingClientRect()` turns "looks off" into a specific number, which
  is a far more actionable finding than a screenshot alone.
- **CSS custom properties, when a component moved hosts.** A token declared on one component's
  class resolves to nothing everywhere else, and `calc()` built on an undefined token silently
  becomes `auto` rather than erroring — a control placed by `bottom: calc(...)` then lands at the
  top of its tab. jsdom computes no styles at all, so the whole class of bug is invisible to the
  suite. Read the token off `:root` in the browser and check an element's measured size against
  what the stylesheet claims.
- **Positioning ancestors.** An absolutely-placed child resolves against the nearest positioned
  ancestor; a host that is `position: static` sends it to the viewport, escaping the tab entirely
  and landing on the shell chrome. Verify placement in *every* host a shared component was added
  to, not just the one it came from.
- **Overlay behavior.** Stacking, backdrop, click-outside, `Escape`. At most one floating overlay
  may be open at a time (SNV-FR-56) — if the feature adds one, confirm opening it closes any other.
- **Focus and keyboard.** Where focus lands on open, whether a modal traps it, where it returns on
  close, whether `Tab` order is sane. jsdom's focus model is an approximation; the browser's is the
  truth.
- **Console.** Read `browser_console_messages` at the end. React key warnings, uncaught errors, and
  failed asset loads are real findings the suite silently swallows.

Do **not** re-verify business logic, state transitions, or error branches — Vitest owns those, and
duplicating them here costs time and produces nothing new.

## Separating real findings from harness artifacts

The mock returns data no real backend would, so some of what you see is the *harness's* fault, not
the feature's. Before reporting anything, ask whether it would still happen against the real
backend. An empty list because the mock returned `[]`, a missing field, a control that does nothing
because its command falls through the switch — those are harness gaps. Fix the mock and re-check.

Report a finding only when it survives a correct mock, and note what the mock returned so the
engineer can judge for themselves. If you can't reach the feature at all after honest attempts at
extending the mock, say so plainly and report what blocked you — "I could not reach this surface"
is an accurate, useful result; findings invented from a half-booted app are not.

## Reporting

Group findings by severity, each naming the FR it bears on:

- **must-fix** — unreachable, visually broken, or violates a spec requirement in the real browser.
- **should-fix** — works, but the interaction is degraded (focus lost, awkward overflow, console
  errors).
- **note** — observations that don't demand action.

Screenshot anything visual you report; a description of a layout bug is much weaker evidence than
the picture. **Give coordinates as well** — a `getBoundingClientRect()` pair and an
`elementFromPoint()` at the disputed spot is what turns a report into something the engineer can fix
without reproducing it, and what lets the fix be re-checked mechanically. State which in-scope FRs
you confirmed working, so silence isn't mistaken for coverage. Mention any `mock-core.ts` cases you
added, so the engineer knows the harness changed.
