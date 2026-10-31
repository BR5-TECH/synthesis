/**
 * VOM-FR-TRWN, VOM-FR-QOXX: what is spoken of a comment's Markdown.
 */
import { stripCitationMarkers } from "../text/citationMarkers";

export const CODE_PHRASE = "Code block.";
export const TABLE_PHRASE = "Table.";

/** VOP-FR-MOSY: the limit of the speech routes. */
export const SEGMENT_LIMIT = 4096;
/** VOM-FR-QOXX: a short first segment makes speech start soon. */
const FIRST_SEGMENT_TARGET = 240;

const FENCE = /^\s*(```|~~~)/;
const TABLE_RULE = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/;

function inlineText(line: string): string {
  return line
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/<[^>\n]+>/g, "")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/(\*\*|__)(.+?)\1/g, "$2")
    .replace(/(\*|_)(\S(?:.*?\S)?)\1/g, "$2")
    .replace(/~~(.+?)~~/g, "$1")
    .replace(/^\s{0,3}#{1,6}\s+/, "")
    .replace(/^\s{0,3}>\s?/, "")
    .replace(/^\s*(?:[-*+]|\d+[.)])\s+(?:\[[ xX]\]\s+)?/, "")
    .trim();
}

function isTableRow(line: string): boolean {
  return /^\s*\|.*\|\s*$/.test(line);
}

/** The text of the Markdown, with code and tables as a short phrase. */
export function speakableText(markdown: string): string {
  const source = stripCitationMarkers(markdown);
  const out: string[] = [];
  let inFence = false;
  let inTable = false;
  for (const line of source.split(/\r?\n/)) {
    if (FENCE.test(line)) {
      if (!inFence) out.push(CODE_PHRASE);
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (isTableRow(line) || (inTable && TABLE_RULE.test(line))) {
      if (!inTable) out.push(TABLE_PHRASE);
      inTable = true;
      continue;
    }
    inTable = false;
    const text = inlineText(line);
    if (text !== "") out.push(text);
  }
  return out.join("\n").trim();
}

function sentences(text: string): string[] {
  return text.match(/[^.!?\n]+(?:[.!?]+(?=\s|$)|\n|$)|[.!?]+/g)?.map((s) => s.trim()) ?? [];
}

function hardSplit(sentence: string): string[] {
  const parts: string[] = [];
  for (let at = 0; at < sentence.length; at += SEGMENT_LIMIT - 1) {
    parts.push(sentence.slice(at, at + SEGMENT_LIMIT - 1));
  }
  return parts;
}

/** VOM-FR-QOXX: split at sentence ends. The first segment is short. */
export function speechSegments(text: string): string[] {
  const segments: string[] = [];
  let current = "";
  for (const sentence of sentences(text).filter((s) => s !== "").flatMap(hardSplit)) {
    const limit = segments.length === 0 ? FIRST_SEGMENT_TARGET : SEGMENT_LIMIT - 1;
    const joined = current === "" ? sentence : `${current} ${sentence}`;
    if (joined.length > limit && current !== "") {
      segments.push(current);
      current = sentence;
    } else {
      current = joined;
    }
  }
  if (current !== "") segments.push(current);
  return segments;
}
