/*
 * The specification map stand-in (OSI-openspec-spec-index.md OSI-FR-OSWD,
 * SMP-specification-map.md SMP-FR-HWIC, SMP-FR-MNZW), for UI auditing only.
 *
 * With no flag, `load_specification_map` answers `{ source: "demo" }`, so the
 * Map tab shows its built-in demo index. Flags:
 *
 *   ?openspecMap               the OpenSpec index below: the root group
 *                              `specs` and the group `billing`, a planned
 *                              capability `ui`, `gap` requirements, and change
 *                              chips from the active worktree and the streams
 *                              s-1 (editor-work) and s-2 (registry-migration)
 *   ?openspecMap=unavailable   the same index with `validation: "unavailable"`
 *                              and one unread location (SMP-FR-KVKD)
 *   ?openspecMap=cli_unavailable
 *                              every load rejects with `cli_unavailable`
 *                              (SMP-FR-KBXW); any other value rejects with it
 *
 * Open the Map tab with View map in the Project panel. The chip
 * `add-logout-3f2a` on editor-work opens the demo change of `mock-openspec.ts`.
 */

import { NOT_HANDLED } from "./mock-github-polling";
import { OPENSPEC_DEMO_CHANGE, OPENSPEC_DEMO_STREAM } from "./mock-openspec";

const LEVELS = [
  { plural: "groups", singular: "group" },
  { plural: "capabilities", singular: "capability" },
  { plural: "requirements", singular: "requirement" },
];

type Issue = { level: "ERROR" | "WARNING" | "INFO"; message: string };

function requirement(
  capability: string,
  n: number,
  label: string,
  summary: string,
  scenarios: number,
  issues?: Issue[],
) {
  return {
    code: `${capability}#${n}`,
    badge: `R${n}`,
    path: `openspec/specs/${capability}/spec.md`,
    label,
    summary,
    requirements: 1,
    scenarios,
    state: issues && issues.some((i) => i.level !== "INFO") ? "gap" : "built",
    ...(issues ? { issues } : {}),
  };
}

const ACTIVE = { kind: "active_worktree" };
const EDITOR_WORK = {
  kind: "stream",
  streamId: OPENSPEC_DEMO_STREAM.id,
  streamName: OPENSPEC_DEMO_STREAM.name,
};
const REGISTRY = { kind: "stream", streamId: "s-2", streamName: "registry-migration" };

function openspecIndex(unavailable: boolean) {
  const issue = (level: Issue["level"], message: string): Issue[] | undefined =>
    unavailable ? undefined : [{ level, message }];
  return {
    levels: LEVELS,
    roots: [
      {
        id: "group:/",
        label: "specs",
        summary: "",
        children: [
          {
            id: "capability:auth",
            label: "Authentication",
            summary: "How a user signs in, how a session lasts, and how it ends.",
            children: [
              requirement("auth", 1, "Session Expiration", "Sessions SHALL expire after 60 minutes.", 1),
              requirement("auth", 2, "Login", "Users SHALL log in with a password.", 2),
              requirement(
                "auth",
                3,
                "Remember Me",
                "Sessions SHALL survive an application restart when Remember me is set.",
                0,
                issue("WARNING", "Requirement should have at least one scenario"),
              ),
            ],
            changes: [
              {
                change: OPENSPEC_DEMO_CHANGE,
                location: ACTIVE,
                ...(unavailable
                  ? {}
                  : { issues: [{ level: "ERROR", message: "Delta Logout has no scenario" }] }),
              },
              { change: OPENSPEC_DEMO_CHANGE, location: EDITOR_WORK },
            ],
          },
          {
            id: "capability:logging",
            label: "Logging",
            summary: "What the application records about itself, and where.",
            children: [
              requirement("logging", 1, "Session Log", "The application SHALL keep one log for each session.", 2),
              requirement("logging", 2, "No Secrets", "A log record SHALL hold no credential.", 1),
            ],
          },
          {
            id: "capability:ui",
            label: "ui",
            summary: "",
            planned: true,
            children: [],
            changes: [
              { change: OPENSPEC_DEMO_CHANGE, location: ACTIVE },
              { change: OPENSPEC_DEMO_CHANGE, location: EDITOR_WORK },
            ],
          },
        ],
      },
      {
        id: "group:billing",
        label: "billing",
        summary: "",
        children: [
          {
            id: "capability:billing/invoices",
            label: "Invoices",
            summary: "The invoice a customer gets for each period.",
            children: [
              requirement("billing/invoices", 1, "Monthly Invoice", "The system SHALL send one invoice each month.", 3),
              requirement("billing/invoices", 2, "Invoice Currency", "An invoice SHALL use the currency of the account.", 1),
            ],
          },
          {
            id: "capability:billing/payments",
            label: "Payments",
            summary: "How a customer pays an invoice.",
            issues: issue("INFO", "Purpose section is short"),
            children: [
              requirement(
                "billing/payments",
                1,
                "Card Payment",
                "A customer SHALL pay an invoice by card.",
                0,
                issue("ERROR", "Requirement must have at least one scenario"),
              ),
              requirement("billing/payments", 2, "Refund", "A customer SHALL get a refund within 14 days.", 2),
            ],
            changes: [{ change: "add-refunds", location: REGISTRY }],
          },
        ],
      },
    ],
    dependencies: [],
    validation: unavailable ? "unavailable" : "invalid",
    unreadLocations: unavailable ? 1 : 0,
  };
}

/** Handle `load_specification_map`, or answer NOT_HANDLED. */
export function specMapInvoke(cmd: string): unknown {
  if (cmd !== "load_specification_map") return NOT_HANDLED;
  const params = typeof location === "undefined" ? null : new URLSearchParams(location.search);
  if (!params?.has("openspecMap")) return { source: "demo" };
  const mode = params.get("openspecMap") ?? "";
  if (mode === "" || mode === "unavailable") {
    return { source: "openspec", index: openspecIndex(mode === "unavailable") };
  }
  // eslint-disable-next-line no-throw-literal
  throw "cli_unavailable";
}
