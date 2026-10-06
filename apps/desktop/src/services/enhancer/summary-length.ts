import type { Transcript } from "@anlg/plugin-template";

export const MIN_TRANSCRIPT_CHARACTERS_FOR_SUMMARY = 160;
export const MIN_SUMMARY_CHARACTERS = 320;
export const MAX_SUMMARY_GUIDANCE_CHARACTERS = 7_500;
const SECTION_GUIDANCE_CHARACTER_STEP = 2_000;
const MAX_GUIDANCE_SECTIONS = 8;

export const SUMMARY_LENGTH_MODES = ["crisp", "balanced", "detailed"] as const;
export type SummaryLengthMode = (typeof SUMMARY_LENGTH_MODES)[number];
export const DEFAULT_SUMMARY_LENGTH_MODE: SummaryLengthMode = "detailed";

const SUMMARY_LENGTH_RATIOS: Record<SummaryLengthMode, number> = {
  crisp: 0.75,
  balanced: 0.875,
  detailed: 1,
};

const SUMMARY_GUIDANCE_CHARACTER_LIMITS: Record<SummaryLengthMode, number> = {
  crisp: 4_500,
  balanced: 6_000,
  detailed: MAX_SUMMARY_GUIDANCE_CHARACTERS,
};

export type SummaryLengthPolicy = {
  transcriptCharacters: number;
  guidance?: {
    maxCharacters: number;
    minSections: number;
    maxSections: number;
  };
};

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function countNormalizedCharacters(text: string): number {
  return Array.from(text.replace(/\s+/gu, " ").trim()).length;
}

export function countTranscriptWordCharacters(
  transcripts: ReadonlyArray<{
    words: ReadonlyArray<{ text?: unknown }>;
  }>,
): number {
  return countNormalizedCharacters(
    transcripts
      .flatMap((transcript) => transcript.words)
      .map((word) => (typeof word.text === "string" ? word.text : ""))
      .filter(Boolean)
      .join(" "),
  );
}

export function getSummaryLengthPolicy(
  transcripts: readonly Transcript[],
  mode: SummaryLengthMode = DEFAULT_SUMMARY_LENGTH_MODE,
): SummaryLengthPolicy | null {
  const transcriptCharacters = countNormalizedCharacters(
    transcripts
      .flatMap((transcript) => transcript.segments)
      .map((segment) => segment.text)
      .filter(Boolean)
      .join(" "),
  );

  if (transcriptCharacters === 0) {
    return null;
  }

  const ratio = SUMMARY_LENGTH_RATIOS[mode];
  const baseMinSections = clamp(
    Math.ceil(transcriptCharacters / (SECTION_GUIDANCE_CHARACTER_STEP * 2)),
    1,
    5,
  );
  const baseMaxSections = clamp(
    1 + Math.ceil(transcriptCharacters / SECTION_GUIDANCE_CHARACTER_STEP),
    2,
    MAX_GUIDANCE_SECTIONS,
  );

  return {
    transcriptCharacters,
    guidance: {
      maxCharacters: clamp(
        Math.round(transcriptCharacters * ratio),
        MIN_SUMMARY_CHARACTERS,
        SUMMARY_GUIDANCE_CHARACTER_LIMITS[mode],
      ),
      minSections: Math.ceil(baseMinSections * ratio),
      maxSections: Math.ceil(baseMaxSections * ratio),
    },
  };
}

export function normalizeSummaryLengthMode(value: unknown): SummaryLengthMode {
  return SUMMARY_LENGTH_MODES.includes(value as SummaryLengthMode)
    ? (value as SummaryLengthMode)
    : DEFAULT_SUMMARY_LENGTH_MODE;
}

export function formatSummaryLengthModeGuidance(
  mode: SummaryLengthMode,
  hasTemplateSections: boolean,
): string {
  const templateGuidance = hasTemplateSections
    ? "Preserve every requested template section and do not add sections based on this mode."
    : "Put only explicitly stated or unambiguous owners, commitments, and deadlines in a final # Next Steps section when any exist; do not turn proposals into commitments, and count Next Steps within the overall section limit.";
  const listGuidance =
    "Write all section content as unordered Markdown list items beginning with '- '; never put prose paragraphs under a heading.";

  if (mode === "crisp") {
    return [
      "Summary mode: crisp. Make the summary fast to scan.",
      "Cover only decisions, outcomes, blockers, commitments, and the context required to understand them.",
      "Do not omit any explicit decision, blocker, owner, commitment, or deadline.",
      "Use direct one-sentence bullets with one idea per bullet and no more than four bullets per section; a section may have fewer than three bullets.",
      "Merge closely related topics into one section before omitting secondary discussion, repetition, conversational framing, minor examples, and rationale that did not affect the outcome.",
      listGuidance,
      templateGuidance,
    ].join(" ");
  }

  if (mode === "balanced") {
    return [
      "Summary mode: balanced. Keep the primary discussion complete while remaining concise.",
      "Do not omit any explicit decision, blocker, owner, commitment, or deadline.",
      "Include important supporting context and rationale, but omit repetition, tangents, and minor examples.",
      "Use three to five direct bullets per section with one or two sentences per bullet.",
      listGuidance,
      templateGuidance,
    ].join(" ");
  }

  return [
    "Summary mode: detailed. Capture every material topic, decision, rationale, example, open question, and commitment.",
    "Use four to seven concrete bullets per section when the source supports it, with one to three sentences and enough context to stand on their own.",
    "Retain useful secondary discussion and examples, but remove repetition and conversational filler.",
    listGuidance,
    templateGuidance,
  ].join(" ");
}

export function formatSummaryLengthGuidance(
  policy: SummaryLengthPolicy | null,
): string | null {
  const guidance = policy?.guidance;
  if (!policy || !guidance) {
    return null;
  }

  const sections =
    guidance.minSections === guidance.maxSections
      ? `exactly ${guidance.maxSections} section${guidance.maxSections === 1 ? "" : "s"}`
      : `${guidance.minSections} to ${guidance.maxSections} sections`;

  return [
    `Summary length: the transcript contains about ${policy.transcriptCharacters} characters.`,
    `Keep the summary proportional to it: use ${sections} and stay under ${guidance.maxCharacters} characters overall.`,
    "A short meeting must produce a short summary; never pad with filler.",
  ].join(" ");
}
