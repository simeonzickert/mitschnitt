import { describe, expect, it } from "vitest";

import {
  countTranscriptWordCharacters,
  formatSummaryLengthModeGuidance,
  formatSummaryLengthGuidance,
  getSummaryLengthPolicy,
  normalizeSummaryLengthMode,
} from "./summary-length";

describe("summary length policy", () => {
  it("counts transcript characters across languages without relying on spaces", () => {
    expect(
      countTranscriptWordCharacters([
        { words: [{ text: "이번" }, { text: "회의는" }, { text: "짧음" }] },
      ]),
    ).toBe(9);
  });

  it("sizes the prompt guidance for short transcripts", () => {
    const policy = getSummaryLengthPolicy([
      {
        startedAt: null,
        endedAt: null,
        segments: [
          { speaker: "John", text: "a".repeat(200), startLabel: null },
        ],
      },
    ]);

    expect(policy).toEqual({
      transcriptCharacters: 200,
      guidance: {
        maxCharacters: 320,
        minSections: 1,
        maxSections: 2,
      },
    });
  });

  it("scales the guided section range with the transcript size", () => {
    const policyFor = (characters: number) =>
      getSummaryLengthPolicy([
        {
          startedAt: null,
          endedAt: null,
          segments: [
            { speaker: "John", text: "a".repeat(characters), startLabel: null },
          ],
        },
      ])?.guidance;

    expect(policyFor(636)).toEqual({
      maxCharacters: 636,
      minSections: 1,
      maxSections: 2,
    });
    expect(policyFor(6_000)).toEqual({
      maxCharacters: 6_000,
      minSections: 2,
      maxSections: 4,
    });
    expect(policyFor(30_000)).toEqual({
      maxCharacters: 7_500,
      minSections: 5,
      maxSections: 8,
    });
  });

  it("renders proportional length guidance for the prompt", () => {
    const policy = getSummaryLengthPolicy([
      {
        startedAt: null,
        endedAt: null,
        segments: [
          { speaker: "John", text: "a".repeat(636), startLabel: null },
        ],
      },
    ]);

    const guidance = formatSummaryLengthGuidance(policy);

    expect(guidance).toContain("about 636 characters");
    expect(guidance).toContain("1 to 2 sections");
    expect(guidance).toContain("under 636 characters");
    expect(formatSummaryLengthGuidance(null)).toBeNull();
  });

  it("keeps the transcript length for long transcripts", () => {
    const policy = getSummaryLengthPolicy([
      {
        startedAt: null,
        endedAt: null,
        segments: [
          { speaker: "John", text: "a".repeat(10_000), startLabel: null },
        ],
      },
    ]);

    expect(policy).toMatchObject({
      transcriptCharacters: 10_000,
    });
  });

  it("reduces prompt guidance for balanced and crisp modes", () => {
    const transcripts = [
      {
        startedAt: null,
        endedAt: null,
        segments: [
          { speaker: "John", text: "a".repeat(10_000), startLabel: null },
        ],
      },
    ];

    expect(getSummaryLengthPolicy(transcripts, "detailed")).toMatchObject({
      guidance: { maxCharacters: 7_500, minSections: 3, maxSections: 6 },
    });
    expect(getSummaryLengthPolicy(transcripts, "balanced")).toMatchObject({
      guidance: { maxCharacters: 6_000, minSections: 3, maxSections: 6 },
    });
    expect(getSummaryLengthPolicy(transcripts, "crisp")).toMatchObject({
      guidance: { maxCharacters: 4_500, minSections: 3, maxSections: 5 },
    });
  });

  it("keeps detailed as the default and explicitly requests full context", () => {
    expect(normalizeSummaryLengthMode(undefined)).toBe("detailed");
    expect(normalizeSummaryLengthMode("unsupported")).toBe("detailed");
    expect(normalizeSummaryLengthMode("crisp")).toBe("crisp");
    expect(formatSummaryLengthModeGuidance("detailed", false)).toContain(
      "every material topic",
    );
  });

  it("guides crisp summaries toward short bullets and explicit follow-ups", () => {
    const guidance = formatSummaryLengthModeGuidance("crisp", false);

    expect(guidance).toContain("one idea per bullet");
    expect(guidance).toContain("# Next Steps");
    expect(formatSummaryLengthModeGuidance("crisp", true)).toContain(
      "Preserve every requested template section",
    );
  });
});
