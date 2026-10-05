/**
 * The pure rules the graduation surfaces render from
 * (`../specifications/ui/GRU-graduation-runs.md`).
 *
 * Everything here is a function of a run as the backend reports it. Nothing
 * decides a run's state or judges what a turn wrote — those are
 * `GRD-graduation.md`'s — and nothing here holds state of its own, which is what
 * lets the two surfaces agree about what a run affords without either owning the
 * answer.
 */

export * from "./runState";
export * from "./merge";
export * from "./blockers";
export * from "./escalations";
export * from "./messages";
export * from "./stages";
export * from "./restart";
export * from "./standingWork";
export * from "./arrangement";
export * from "./coalesce";
export * from "./pathTree";
export * from "./pathsWidth";
export * from "./slotWait";
