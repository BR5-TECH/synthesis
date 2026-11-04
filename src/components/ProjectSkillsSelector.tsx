/**
 * The Project skills selector of a Project agent's persona editor
 * (`specifications/ui/AGT-agents.md` AGT-FR-KVDA, AGT-FR-XGWD, AGT-FR-LCMT).
 *
 * A project skill reaches an agent only when the author selects it here, by
 * its path, because a skill's name is not its identity (AGR-FR-IGIL). The
 * built-in skills of the application reach every agent and are not listed.
 * Nothing is written from here: the selection travels with the editor's
 * confirm (AGT-FR-19).
 */
import { useEffect, useState } from "react";
import * as api from "../api";
import { logWarn } from "../logging";
import type { ProjectSkill } from "../types";

/** AGT-FR-KVDA: the statement about the built-in skills. */
export const BUILTIN_SKILLS_NOTE =
  "Built-in skills are always available to every agent.";

/** AGT-FR-LCMT: where a project skill is found. */
export const PROJECT_SKILL_FOLDERS = [
  ".claude/skills",
  ".codex/skills",
  ".github/skills",
  ".opencode/skills",
];

interface ProjectSkillsSelectorProps {
  /** The selected paths, in the order they were selected. */
  selected: string[];
  onChange: (selected: string[]) => void;
}

/** One entry of the list: a skill the registry holds, or a stored path that
 * it does not (AGT-FR-XGWD). */
interface Entry {
  path: string;
  name: string | null;
}

/** AGT-FR-KVDA / AGT-FR-XGWD: the registry's skills in path order, then the
 * stored paths the registry does not hold, so the author sees and can clear
 * every selected path. */
export function selectorEntries(skills: ProjectSkill[], selected: string[]): Entry[] {
  const known = new Set(skills.map((s) => s.path));
  const listed: Entry[] = skills.map((s) => ({ path: s.path, name: s.name }));
  const missing: Entry[] = selected
    .filter((path) => !known.has(path))
    .map((path) => ({ path, name: null }));
  return [...listed, ...missing];
}

export function ProjectSkillsSelector({ selected, onChange }: ProjectSkillsSelectorProps) {
  // `undefined` while the read is in flight, `null` when it failed.
  const [skills, setSkills] = useState<ProjectSkill[] | null | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    api
      .listProjectSkills()
      .then((list) => {
        if (!cancelled) setSkills(list ?? []);
      })
      .catch((e) => {
        // AGT-FR-LCMT: the stored selection stays, and the confirm action
        // stays enabled. Only the shape of the failure is logged.
        logWarn(["frontend"], "could not read the project skills", {
          reason: typeof e === "string" ? e : "unknown",
        });
        if (!cancelled) setSkills(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = (path: string, on: boolean) => {
    if (on) {
      if (!selected.includes(path)) onChange([...selected, path]);
    } else {
      onChange(selected.filter((p) => p !== path));
    }
  };

  const entries = selectorEntries(skills ?? [], selected);

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span className="t-ui-sm" id="agent-skills-label">
        Project skills
      </span>
      {skills === undefined ? (
        <span className="t-ui-sm t-muted" data-testid="agent-skills-loading">
          Reading the project skills…
        </span>
      ) : (
        <>
          {skills === null && (
            <span className="picker-error" data-testid="agent-skills-error">
              The project skills could not be read. The selection stays as it is.
            </span>
          )}
          {entries.length === 0 && skills !== null ? (
            <span className="t-ui-sm t-muted" data-testid="agent-skills-empty">
              This project has no skills. A project skill is a folder with a
              SKILL.md in {PROJECT_SKILL_FOLDERS.join(", ")}.
            </span>
          ) : (
            entries.length > 0 && (
              <div
                role="group"
                aria-labelledby="agent-skills-label"
                data-testid="agent-skills"
                style={{
                  display: "flex",
                  flexDirection: "column",
                  gap: 2,
                  maxHeight: 168,
                  overflowY: "auto",
                  padding: "6px 8px",
                  border: "1px solid var(--border-1)",
                  borderRadius: 6,
                  scrollMarginBottom: 72,
                }}
              >
                {entries.map((entry) => (
                  <label
                    key={entry.path}
                    className="t-ui-sm"
                    data-testid="agent-skill-option"
                    data-path={entry.path}
                    style={{
                      display: "flex",
                      gap: 8,
                      alignItems: "baseline",
                      minWidth: 0,
                      padding: "2px 0",
                    }}
                  >
                    <input
                      type="checkbox"
                      checked={selected.includes(entry.path)}
                      onChange={(e) => toggle(entry.path, e.target.checked)}
                      style={{ scrollMarginBottom: 72, flexShrink: 0 }}
                    />
                    <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
                      <span style={{ color: "var(--fg-1)" }}>
                        {entry.name ?? entry.path.split("/").slice(-2, -1)[0] ?? entry.path}
                      </span>
                      <span
                        className="t-meta"
                        style={{ overflowWrap: "anywhere" }}
                        title={entry.path}
                      >
                        {entry.path}
                      </span>
                      {entry.name === null && skills !== null && (
                        <span className="t-ui-xs t-muted" data-testid="agent-skill-missing">
                          Not found in this worktree.
                        </span>
                      )}
                    </span>
                  </label>
                ))}
              </div>
            )
          )}
        </>
      )}
      <span className="t-ui-sm t-muted" data-testid="agent-skills-builtin-note">
        {BUILTIN_SKILLS_NOTE}
      </span>
    </div>
  );
}
