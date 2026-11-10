/**
 * One demo change for the tests of the change workspace: four artifacts, and
 * one delta of each operation, with and without a live requirement.
 */
import type { ChangeDetail, Requirement } from "../../types";

export const LIVE_EXPIRY: Requirement = {
  name: "Session Expiration",
  text: "Sessions SHALL expire after 60 minutes.",
  scenarios: [{ name: "Idle", text: "- **WHEN** a session is idle for 60 minutes\n- **THEN** it expires" }],
};

export const CHANGED_EXPIRY: Requirement = {
  name: "Session Expiration",
  text: "Sessions SHALL expire after 30 minutes of no activity.",
  scenarios: [{ name: "Idle", text: "- **WHEN** a session is idle for 30 minutes\n- **THEN** it expires" }],
};

export function demoChange(over: Partial<ChangeDetail> = {}): ChangeDetail {
  return {
    name: "add-logout-3f2a",
    location: { kind: "stream", streamId: "s-1" },
    schema: "spec-driven",
    isComplete: false,
    completedTasks: 1,
    totalTasks: 4,
    artifacts: [
      {
        id: "proposal",
        status: "done",
        files: [
          {
            path: "proposal.md",
            text: "# Why\n\nUsers <b>cannot</b> log out. See [the docs](https://example.com).\n\n![remote](https://example.com/x.png)",
            unreadable: false,
          },
        ],
      },
      {
        id: "specs",
        status: "done",
        files: [
          { path: "specs/auth/spec.md", text: "## MODIFIED Requirements", unreadable: false },
          { path: "specs/ui/spec.md", text: "## ADDED Requirements", unreadable: false },
        ],
      },
      {
        id: "design",
        status: "ready",
        files: [{ path: "design.md", text: null, unreadable: true }],
      },
      { id: "tasks", status: "blocked", files: [] },
    ],
    deltas: [
      {
        capability: "auth",
        capabilityExists: true,
        operation: "MODIFIED",
        requirement: CHANGED_EXPIRY,
        rename: null,
        live: LIVE_EXPIRY,
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "ADDED",
        requirement: {
          name: "Logout",
          text: "Users SHALL log out from every page.",
          scenarios: [{ name: "Menu", text: "- **WHEN** the user picks Log out\n- **THEN** the session ends" }],
        },
        rename: null,
        live: null,
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "REMOVED",
        requirement: { name: "Remember me", text: "", scenarios: [] },
        rename: null,
        live: { name: "Remember me", text: "Sessions SHALL survive a restart.", scenarios: [] },
      },
      {
        capability: "ui",
        capabilityExists: false,
        operation: "MODIFIED",
        requirement: { name: "Header", text: "The header SHALL show a Log out button.", scenarios: [] },
        rename: null,
        live: null,
      },
      {
        capability: "ui",
        capabilityExists: false,
        operation: "REMOVED",
        requirement: { name: "Footer link", text: "", scenarios: [] },
        rename: null,
        live: null,
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "RENAMED",
        requirement: null,
        rename: { from: "Login", to: "Sign in" },
        live: { name: "Login", text: "Users SHALL log in with a password.", scenarios: [] },
      },
    ],
    ...over,
  };
}
