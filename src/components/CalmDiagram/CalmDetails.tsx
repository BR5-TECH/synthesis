/**
 * CAD-FR-KUBU – CAD-FR-MYHZ: the details panel of an interactive CALM canvas.
 * It shows the selected item, or the chosen flow, or a summary of the
 * architecture with its Nodes list. Every CALM string is plain text, and a URL
 * is text that the author can select, never a link (CLM-FR-IRGG).
 */
import type { ReactNode } from "react";

import type {
  CalmControl,
  CalmDecision,
  CalmFlowView,
  CalmGraph,
  JsonObject,
} from "../../calm/types";
import type { CalmSelection } from "./model";
import { ShapeGlyph } from "./nodes";

const RELATIONSHIP_NAMES: Record<string, string> = {
  connects: "connects",
  interacts: "interacts",
  "deployed-in": "deployed-in",
  "composed-of": "composed-of",
  options: "options",
};

/** CAD-FR-MISQ: a value; an object or an array is indented JSON text. */
function Value({ value }: { value: unknown }) {
  if (value !== null && typeof value === "object") {
    return <pre className="calm-details__json">{JSON.stringify(value, null, 2)}</pre>;
  }
  return <span className="calm-details__value">{String(value)}</span>;
}

/** CAD-FR-MISQ: each property of an object as a key and a value. */
function Pairs({ value }: { value: unknown }) {
  if (value === undefined || value === null) return null;
  const entries: Array<[string, unknown]> = Array.isArray(value)
    ? value.flatMap((item) =>
        item !== null && typeof item === "object" ? Object.entries(item as JsonObject) : [],
      )
    : typeof value === "object"
      ? Object.entries(value as JsonObject)
      : [["value", value]];
  if (entries.length === 0) return null;
  return (
    <dl className="calm-details__pairs">
      {entries.map(([key, item], index) => (
        <div key={`${key}:${index}`} className="calm-details__pair">
          <dt>{key}</dt>
          <dd>
            <Value value={item} />
          </dd>
        </div>
      ))}
    </dl>
  );
}

/** A section with a heading; a section with no content does not show. */
function Section({ title, children }: { title: string; children: ReactNode }) {
  if (children === null || children === false) return null;
  return (
    <section className="calm-details__section" aria-label={title}>
      <h4 className="calm-details__section-title">{title}</h4>
      {children}
    </section>
  );
}

function Interfaces({ interfaces }: { interfaces: JsonObject[] }) {
  if (interfaces.length === 0) return null;
  return (
    <Section title="Interfaces">
      {interfaces.map((iface, index) => (
        <div key={index} className="calm-details__item">
          <div className="calm-details__item-name">{String(iface["unique-id"] ?? "")}</div>
          <Pairs
            value={Object.fromEntries(Object.entries(iface).filter(([key]) => key !== "unique-id"))}
          />
        </div>
      ))}
    </Section>
  );
}

/** CAD-FR-MISQ: each control with its description, requirement, and configuration. */
function Controls({ controls }: { controls: CalmControl[] }) {
  if (controls.length === 0) return null;
  return (
    <Section title="Controls">
      {controls.map((control) => (
        <div key={control.id} className="calm-details__item">
          <div className="calm-details__item-name">{control.id}</div>
          {control.description && <p>{control.description}</p>}
          {control.requirements.map((requirement, index) => (
            <dl key={index} className="calm-details__pairs">
              <div className="calm-details__pair">
                <dt>Requirement</dt>
                <dd className="calm-details__url">{requirement.requirementUrl}</dd>
              </div>
              {requirement.configUrl !== undefined && (
                <div className="calm-details__pair">
                  <dt>Configuration</dt>
                  <dd className="calm-details__url">{requirement.configUrl}</dd>
                </div>
              )}
              {requirement.config !== undefined &&
                Object.entries(
                  requirement.config !== null && typeof requirement.config === "object"
                    ? (requirement.config as JsonObject)
                    : { config: requirement.config },
                ).map(([key, value]) => (
                  <div key={key} className="calm-details__pair">
                    <dt>{key}</dt>
                    <dd>
                      <Value value={value} />
                    </dd>
                  </div>
                ))}
            </dl>
          ))}
        </div>
      ))}
    </Section>
  );
}

function Metadata({ metadata }: { metadata: unknown }) {
  if (metadata === undefined || metadata === null) return null;
  return (
    <Section title="Metadata">
      <Pairs value={metadata} />
    </Section>
  );
}

function Decisions({ decisions }: { decisions: CalmDecision[] }) {
  if (decisions.length === 0) return null;
  return (
    <Section title="Decisions">
      <ul className="calm-details__list">
        {decisions.map((decision, index) => (
          <li key={index}>
            {decision.description}
            <span className="calm-details__muted"> ({decision.relationshipId})</span>
          </li>
        ))}
      </ul>
    </Section>
  );
}

function Header({ title, id, type, description }: { title: string; id: string; type: string; description: string }) {
  return (
    <header className="calm-details__header">
      <h3 className="calm-details__title">{title}</h3>
      <div className="calm-details__id">
        <span>{id}</span>
        <span className="calm-details__type">{type}</span>
      </div>
      {description && <p>{description}</p>}
    </header>
  );
}

export interface CalmDetailsProps {
  graph: CalmGraph;
  selected: CalmSelection | null;
  flow: CalmFlowView | null;
  /** CAD-FR-MXHO: selects a node or the edge of a step from the panel. */
  onSelect: (selection: CalmSelection) => void;
}

const nameOf = (graph: CalmGraph, id: string) =>
  graph.nodes.find((node) => node.id === id)?.name ?? id;

export function CalmDetails({ graph, selected, flow, onSelect }: CalmDetailsProps) {
  let body: ReactNode;
  const node = selected?.kind === "node" ? graph.nodes.find((n) => n.id === selected.id) : undefined;
  const edge = selected?.kind === "edge" ? graph.edges.find((e) => e.id === selected.id) : undefined;
  const relationship = edge
    ? graph.relationships.find((rel) => rel.id === edge.relationshipId)
    : undefined;
  if (node) {
    // CAD-FR-LQZM
    body = (
      <>
        <Header title={node.name} id={node.id} type={node.nodeType} description={node.description} />
        <Interfaces interfaces={node.interfaces} />
        <Controls controls={node.controls} />
        <Metadata metadata={node.metadata} />
        <Decisions decisions={node.decisions} />
      </>
    );
  } else if (edge && relationship) {
    // CAD-FR-MDTX
    const ends =
      relationship.sourceInterfaces.length > 0 || relationship.targetInterfaces.length > 0;
    body = (
      <>
        <Header
          title={`${nameOf(graph, edge.source)} → ${nameOf(graph, edge.target)}`}
          id={relationship.id}
          type={RELATIONSHIP_NAMES[relationship.type] ?? relationship.type}
          description={relationship.description}
        />
        {relationship.protocol !== undefined && (
          <Section title="Protocol">
            <span className="calm-chip">{relationship.protocol}</span>
          </Section>
        )}
        {ends && (
          <Section title="Interfaces">
            <dl className="calm-details__pairs">
              {relationship.sourceInterfaces.length > 0 && (
                <div className="calm-details__pair">
                  <dt>{nameOf(graph, edge.source)}</dt>
                  <dd>{relationship.sourceInterfaces.join(", ")}</dd>
                </div>
              )}
              {relationship.targetInterfaces.length > 0 && (
                <div className="calm-details__pair">
                  <dt>{nameOf(graph, edge.target)}</dt>
                  <dd>{relationship.targetInterfaces.join(", ")}</dd>
                </div>
              )}
            </dl>
          </Section>
        )}
        <Controls controls={relationship.controls} />
        <Metadata metadata={relationship.metadata} />
        <Decisions decisions={relationship.decisions} />
      </>
    );
  } else if (flow) {
    // CAD-FR-MJWJ, CAD-FR-MXHO
    body = (
      <>
        <header className="calm-details__header">
          <h3 className="calm-details__title">{flow.name}</h3>
          {flow.description && <p>{flow.description}</p>}
        </header>
        <Section title="Steps">
          <ol className="calm-details__steps">
            {flow.steps.map((step, index) => {
              const names = (ids: string[]) => ids.map((id) => nameOf(graph, id)).join(", ");
              // CAD-FR-MJWJ: the two nodes a step joins, or the nodes that a
              // step with no edge highlights (CLM-FR-FSCM).
              const label =
                step.edgeIds.length > 0
                  ? `${names(step.from)} → ${names(step.to)}`
                  : step.nodeIds.length > 0
                    ? names(step.nodeIds)
                    : step.relationshipId;
              return (
                <li key={index}>
                  <button
                    type="button"
                    className="calm-details__entry"
                    onClick={() => {
                      const target = step.edgeIds[0] ?? step.nodeIds[0];
                      if (target === undefined) return;
                      onSelect(
                        step.edgeIds[0] !== undefined
                          ? { kind: "edge", id: target }
                          : { kind: "node", id: target },
                      );
                    }}
                  >
                    <span className="calm-step">{step.sequence}</span>
                    <span className="calm-details__step-text">
                      <span className="calm-details__step-joins">{label}</span>
                      {step.description && <span>{step.description}</span>}
                    </span>
                  </button>
                </li>
              );
            })}
          </ol>
        </Section>
      </>
    );
  } else {
    // CAD-FR-MUJL, CAD-FR-MXHO
    body = (
      <>
        <header className="calm-details__header">
          <h3 className="calm-details__title">Architecture</h3>
          <p className="calm-details__counts">
            {graph.nodes.length} {graph.nodes.length === 1 ? "node" : "nodes"},{" "}
            {graph.relationships.length}{" "}
            {graph.relationships.length === 1 ? "relationship" : "relationships"},{" "}
            {graph.flows.length} {graph.flows.length === 1 ? "flow" : "flows"}
          </p>
        </header>
        <Metadata metadata={graph.metadata} />
        <Controls controls={graph.controls} />
        {graph.adrs.length > 0 && (
          <Section title="ADRs">
            <ul className="calm-details__list">
              {graph.adrs.map((adr, index) => (
                <li key={index} className="calm-details__url">
                  {adr}
                </li>
              ))}
            </ul>
          </Section>
        )}
        <Section title="Nodes">
          <ul className="calm-details__list">
            {graph.nodes.map((item) => (
              <li key={item.id}>
                <button
                  type="button"
                  className="calm-details__entry"
                  onClick={() => onSelect({ kind: "node", id: item.id })}
                >
                  <ShapeGlyph shape={item.shape} />
                  <span>{item.name}</span>
                </button>
              </li>
            ))}
          </ul>
        </Section>
      </>
    );
  }
  return (
    <aside className="calm-details" aria-label="Details" tabIndex={0}>
      {body}
    </aside>
  );
}
