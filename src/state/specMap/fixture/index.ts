/**
 * SMP-FR-HWIC: the demo index, whole. Its dependencies name only spec codes the
 * demo roots hold, and three of them are unresolved so the map has broken
 * citations to show.
 */
import type { Dependency, SpecificationIndex } from "../types";
import { DEMO_ROOTS } from "./domains";

type Edge = [from: string, to: string, citations: number, unresolved?: boolean];

const EDGES: Edge[] = [
  ["OVW", "SNV", 9], ["OVW", "CVP", 6], ["OVW", "NTF", 3], ["OVW", "DFV", 4], ["OVW", "EDT", 7],
  ["OVW", "GIT", 2], ["OVW", "CHG", 5], ["OVW", "SET", 2], ["OVW", "GHA", 2], ["OVW", "STB", 3],
  ["OVW", "SWN", 6], ["OVW", "TAB", 2], ["OVW", "NFI", 3], ["OVW", "NTA", 3], ["OVW", "NFW", 2],
  ["OVW", "CMW", 2], ["OVW", "DRP", 4], ["OVW", "DSH", 2], ["OVW", "NAW", 3], ["OVW", "LOG", 1],
  ["OVW", "CMP", 1], ["OVW", "WTS", 2], ["OVW", "WTC", 1], ["OVW", "GLS", 1], ["OVW", "FLO", 4],
  ["OVW", "SMP", 2],
  ["FLO", "FGV", 5], ["FLO", "EDT", 3],
  ["STB", "PRG", 5], ["STB", "CHC", 6], ["STB", "PSS", 2], ["STB", "PST", 1], ["STB", "EDT", 6],
  ["STB", "DFV", 3], ["STB", "CVP", 2], ["STB", "GLS", 1], ["STB", "SET", 3], ["STB", "SWN", 4],
  ["STB", "SNV", 6], ["STB", "OVW", 3], ["STB", "CHG", 2],
  ["SNV", "TAB", 7], ["SNV", "LIB", 4], ["SNV", "NTS", 2], ["SNV", "CMP", 2], ["SNV", "DRP", 2],
  ["SNV", "ACT", 3], ["SNV", "SCH", 2],
  ["EDT", "DFV", 8], ["EDT", "TAB", 5], ["EDT", "EFR", 3], ["EDT", "ESH", 2], ["EDT", "EXC", 4], ["EDT", "CMT", 6],
  ["DFV", "CHG", 5], ["CHG", "CHC", 7], ["CHG", "GTC", 4], ["GIT", "GTC", 5], ["GIT", "GHA", 3], ["GHA", "GTS", 4],
  ["WTS", "WTC", 6], ["WTC", "PST", 3], ["CHC", "GTC", 5], ["WSS", "WKS", 5], ["WKS", "WTC", 4],
  ["CMT", "CMS", 9], ["CMP", "CMT", 5], ["CTA", "CMT", 6], ["CTA", "AGC", 4], ["AUC", "CMS", 3], ["ADQ", "DQA", 4],
  ["DRP", "DRS", 8], ["DRP", "DCP", 4], ["DCR", "DCP", 6], ["DCR", "DFV", 3], ["NAW", "DRS", 7], ["NAW", "GRD", 5],
  ["DRS", "DHS", 4], ["DRS", "DAS", 3], ["DSS", "DRS", 3], ["PDC", "DCP", 4], ["PPC", "PCP", 4], ["DFI", "DSS", 3],
  ["PCR", "PCP", 5], ["DDS", "AGC", 4], ["DQA", "DDS", 3],
  ["GRD", "GRL", 11], ["GRD", "GXD", 8], ["GRL", "GLG", 4], ["GXD", "EAC", 6], ["GRB", "GXD", 5],
  ["GHP", "GTC", 4], ["GSU", "GRD", 4], ["GOB", "GRS", 6], ["GRU", "GOB", 5], ["GRH", "GRS", 4],
  ["GLW", "GRS", 3], ["GEA", "ESU", 3], ["RPV", "GOB", 3], ["GSD", "GSU", 4], ["GTE", "GRD", 3], ["RUN", "GRU", 2],
  ["CVL", "AGC", 9], ["CVP", "AGC", 7], ["AGC", "AGR", 4], ["AGV", "AGC", 3], ["AIC", "ADP", 3], ["AGT", "AGR", 5],
  ["AII", "AAP", 6], ["AAP", "ASV", 4], ["EAC", "AIC", 5], ["EAC", "ACM", 3], ["AIC", "AVI", 4],
  ["AVI", "CCP", 3], ["AVI", "CDX", 3],
  ["SPS", "BMI", 4], ["SST", "BMI", 3], ["DST", "BMI", 3], ["NST", "BMI", 3], ["SLT", "DSL", 3],
  ["LSK", "DSL", 3], ["RFT", "FSA", 4], ["RDT", "DRS", 3], ["RGF", "GRS", 3], ["ESU", "TLC", 4],
  ["WFT", "TLC", 2], ["WST", "TLC", 2], ["SPS", "TLC", 3], ["SCH", "SCC", 5], ["SCC", "BMI", 4],
  ["ASC", "FSA", 4], ["BMI", "ASC", 5], ["PSS", "PST", 4], ["GSS", "ASV", 3], ["NTF", "NTD", 5],
  ["LOG", "LGC", 4], ["PLG", "DSL", 2], ["GLS", "GSS", 6], ["SET", "PSS", 6], ["FGV", "PST", 2],
  ["NTS", "NTC", 5], ["RMS", "PST", 2],
  ["SPC", "TSK", 3], ["CIP", "TSK", 4], ["CIP", "ACM", 2], ["BMS", "CIP", 2], ["TSK", "AVI", 3],
  ["SAS", "BMS", 4], ["RSN", "WSK", 5], ["SRB", "WSK", 3], ["RSN", "SRB", 3],
  ["SMP", "TAB", 3], ["SMP", "LIB", 2], ["SMZ", "SMN", 3], ["SME", "SMN", 2], ["SMI", "SMO", 2],
  ["SMD", "DRP", 3], ["SMD", "NAW", 2], ["SMO", "SMN", 2],
  ["LCM", "NAW", 2, true],
  ["ACT", "GRU", 2, true],
  ["DFI", "GRS", 1, true],
];

const DEPENDENCIES: Dependency[] = EDGES.map(([from, to, citations, unresolved]) =>
  unresolved ? { from, to, citations, unresolved } : { from, to, citations },
);

export const DEMO_SPECIFICATION_INDEX: SpecificationIndex = {
  levels: [
    { plural: "domains", singular: "domain" },
    { plural: "features", singular: "feature" },
    { plural: "groups", singular: "group" },
    { plural: "specs", singular: "spec" },
  ],
  roots: DEMO_ROOTS,
  dependencies: DEPENDENCIES,
};
