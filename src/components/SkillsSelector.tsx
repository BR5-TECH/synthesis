/**
 * The Skills selector of the persona editor
 * (`specifications/ui/AGT-agents.md` AGT-FR-KVDA, AGT-FR-PQSM, AGT-FR-JRWE,
 * AGT-FR-XGWD, AGT-FR-LCMT, AGT-FR-WTSR).
 *
 * Two groups. The synthesis skills of `.synthesis/skills` are on for every
 * agent until the author clears one here, so the group edits a list of
 * opt-outs (AGR-FR-HMVQ). The project skills of the four agent-ecosystem
 * folders are off until the author selects one, and only a Project agent can
 * (AGR-FR-IGIL). Every skill is named by its path, because a skill's name is
 * not its identity. The built-in skills reach every agent and are not listed.
 * Nothing is written from here: both lists travel with the editor's confirm
 * (AGT-FR-19).
 */
import { useEffect, useState } from "react";
import * as api from "../api";
import { logWarn } from "../logging";
import type { AgentScope, ProjectSkill } from "../types";

/** AGT-FR-KVDA: the statement about the built-in skills. */
export const BUILTIN_SKILLS_NOTE = "Built-in skills are always available to every agent.";

/** AGT-FR-LCMT: where a synthesis skill is found. */
export const SYNTHESIS_SKILL_FOLDER = ".synthesis/skills";

/** AGT-FR-LCMT: where a project skill is found. */
export const PROJECT_SKILL_FOLDERS = [
  ".claude/skills",
  ".codex/skills",
  ".github/skills",
  ".opencode/skills",
];

/** The refusal of `list_project_skills` with no project open (DSL-FR-PEHR). */
const NO_PROJECT_OPEN = "no_project_open";

interface SkillsSelectorProps {
  scope: AgentScope;
  /** The selected project skill paths, in the order they were selected. */
  selected: string[];
  onSelectedChange: (selected: string[]) => void;
  /** The synthesis skill paths the agent turns off. */
  disabled: string[];
  onDisabledChange: (disabled: string[]) => void;
}

/** One entry of a group: a skill the registry holds, or a stored path that it
 * does not (AGT-FR-XGWD). */
interface Entry {
  path: string;
  name: string | null;
}

/** AGT-FR-KVDA / AGT-FR-XGWD: the project group — the registry's project
 * skills in path order, then the stored paths the registry does not hold, so
 * the author sees and can clear every selected path. */
export function selectorEntries(skills: ProjectSkill[], selected: string[]): Entry[] {
  const projectSkills = skills.filter((s) => s.ecosystem !== "synthesis");
  const known = new Set(projectSkills.map((s) => s.path));
  const listed: Entry[] = projectSkills.map((s) => ({ path: s.path, name: s.name }));
  const missing: Entry[] = selected
    .filter((path) => !known.has(path))
    .map((path) => ({ path, name: null }));
  return [...listed, ...missing];
}

/** AGT-FR-KVDA / AGT-FR-JRWE: the synthesis group — the registry's synthesis
 * skills in path order. A stored opt-out the registry does not hold is not
 * listed: it can belong to another project, and it stays stored as it is. */
export function synthesisEntries(skills: ProjectSkill[]): Entry[] {
  return skills
    .filter((s) => s.ecosystem === "synthesis")
    .map((s) => ({ path: s.path, name: s.name }));
}

/** AGT-FR-PQSM / AGT-FR-JRWE: turn one synthesis skill on or off. Only the
 * toggled path changes, so every other stored path stays. */
export function toggledOptOuts(disabled: string[], path: string, on: boolean): string[] {
  if (on) return disabled.filter((p) => p !== path);
  return disabled.includes(path) ? disabled : [...disabled, path];
}

function readFailure(reason: string | null): string {
  return reason === NO_PROJECT_OPEN
    ? "No project is open, so its skills are not shown. The selections stay as they are."
    : "The skills could not be read. The selections stay as they are.";
}

const groupStyle = {
  display: "flex",
  flexDirection: "column",
  gap: 2,
} as const;

function SkillOption({
  entry,
  checked,
  onToggle,
  missingNote,
}: {
  entry: Entry;
  checked: boolean;
  onToggle: (on: boolean) => void;
  missingNote: boolean;
}) {
  return (
    <label
      className="t-ui-sm"
      data-testid="agent-skill-option"
      data-path={entry.path}
      style={{ display: "flex", gap: 8, alignItems: "baseline", minWidth: 0, padding: "2px 0" }}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onToggle(e.target.checked)}
        style={{ scrollMarginBottom: 72, flexShrink: 0 }}
      />
      <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
        <span style={{ color: "var(--fg-1)" }}>
          {entry.name ?? entry.path.split("/").slice(-2, -1)[0] ?? entry.path}
        </span>
        <span className="t-meta" style={{ overflowWrap: "anywhere" }} title={entry.path}>
          {entry.path}
        </span>
        {missingNote && (
          <span className="t-ui-xs t-muted" data-testid="agent-skill-missing">
            Not found in this worktree.
          </span>
        )}
      </span>
    </label>
  );
}

export function SkillsSelector({
  scope,
  selected,
  onSelectedChange,
  disabled,
  onDisabledChange,
}: SkillsSelectorProps) {
  // `undefined` while the read is in flight, `null` when it failed.
  const [skills, setSkills] = useState<ProjectSkill[] | null | undefined>(undefined);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .listProjectSkills()
      .then((list) => {
        if (!cancelled) setSkills(list ?? []);
      })
      .catch((e) => {
        // AGT-FR-LCMT: both stored lists stay, and the confirm action stays
        // enabled. Only the shape of the failure is logged.
        const reason = typeof e === "string" ? e : null;
        logWarn(["frontend"], "could not read the skills", { reason: reason ?? "unknown" });
        if (!cancelled) {
          setFailure(reason);
          setSkills(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggleProject = (path: string, on: boolean) => {
    if (on) {
      if (!selected.includes(path)) onSelectedChange([...selected, path]);
    } else {
      onSelectedChange(selected.filter((p) => p !== path));
    }
  };

  const synthesis = synthesisEntries(skills ?? []);
  const project = selectorEntries(skills ?? [], selected);
  const showProject = scope === "project";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span className="t-ui-sm" id="agent-skills-label">
        Skills
      </span>
      {skills === undefined ? (
        <span className="t-ui-sm t-muted" data-testid="agent-skills-loading">
          Reading the skills…
        </span>
      ) : (
        <>
          {skills === null && (
            <span className="picker-error" data-testid="agent-skills-error">
              {readFailure(failure)}
            </span>
          )}
          <div
            data-testid="agent-skills"
            aria-labelledby="agent-skills-label"
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 8,
              maxHeight: 280,
              overflowY: "auto",
              padding: "6px 8px",
              border: "1px solid var(--border-1)",
              borderRadius: 6,
              scrollMarginBottom: 72,
            }}
          >
            <div role="group" aria-labelledby="agent-synthesis-skills-label" data-testid="agent-synthesis-skills" style={groupStyle}>
              <span className="t-ui-xs t-muted" id="agent-synthesis-skills-label">
                Synthesis skills — on by default
              </span>
              {synthesis.length === 0 && skills !== null && (
                <span className="t-ui-sm t-muted" data-testid="agent-synthesis-skills-empty">
                  This project has no synthesis skills. A synthesis skill is a folder with a
                  SKILL.md in {SYNTHESIS_SKILL_FOLDER}.
                </span>
              )}
              {synthesis.map((entry) => (
                <SkillOption
                  key={entry.path}
                  entry={entry}
                  checked={!disabled.includes(entry.path)}
                  onToggle={(on) => onDisabledChange(toggledOptOuts(disabled, entry.path, on))}
                  missingNote={false}
                />
              ))}
            </div>
            {showProject && (
              <div role="group" aria-labelledby="agent-project-skills-label" data-testid="agent-project-skills" style={groupStyle}>
                <span className="t-ui-xs t-muted" id="agent-project-skills-label">
                  Project skills — Project agents only
                </span>
                {project.length === 0 && skills !== null && (
                  <span className="t-ui-sm t-muted" data-testid="agent-skills-empty">
                    This project has no project skills. A project skill is a folder with a
                    SKILL.md in {PROJECT_SKILL_FOLDERS.join(", ")}.
                  </span>
                )}
                {project.map((entry) => (
                  <SkillOption
                    key={entry.path}
                    entry={entry}
                    checked={selected.includes(entry.path)}
                    onToggle={(on) => toggleProject(entry.path, on)}
                    missingNote={entry.name === null && skills !== null}
                  />
                ))}
              </div>
            )}
          </div>
        </>
      )}
      <span className="t-ui-sm t-muted" data-testid="agent-skills-builtin-note">
        {BUILTIN_SKILLS_NOTE}
      </span>
    </div>
  );
}
