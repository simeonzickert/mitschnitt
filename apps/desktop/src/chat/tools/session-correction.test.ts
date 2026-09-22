import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { json2md, md2json, parseJsonContent } from "@anlg/editor/markdown";

const mocks = vi.hoisted(() => ({
  applySessionContentCorrections: vi.fn(),
  loadSessionContentSnapshot: vi.fn(),
  updateSettingValue: vi.fn(),
  vocabularyRiskyAliases: vi.fn(),
}));

// Wer eine Verhoerung verwirft, entscheidet Rust. Der Test tut so, als saesse
// dort niemand -- und macht damit sichtbar, dass die Antwort wirklich von dort
// kommt und nicht aus einer Kopie im Frontend.
vi.mock("@anlg/plugin-local-stt", () => ({
  commands: { vocabularyRiskyAliases: mocks.vocabularyRiskyAliases },
}));

vi.mock("~/session/content-mutations", () => ({
  applySessionContentCorrections: mocks.applySessionContentCorrections,
}));

vi.mock("~/session/content-queries", () => ({
  loadSessionContentSnapshot: mocks.loadSessionContentSnapshot,
}));

vi.mock("~/settings/queries", () => ({
  updateSettingValue: mocks.updateSettingValue,
}));

import {
  buildApplySessionCorrectionTool,
  sessionCorrectionTestInternals,
} from "./session-correction";

function summary(markdown: string, id = "note-1", title = "Summary") {
  const content = JSON.stringify(md2json(markdown));
  return {
    id,
    title,
    markdown,
    content,
    contentFormat: "prosemirror_json",
    templateId: "",
    position: 0,
  };
}

function transcript({
  words,
  memo,
}: {
  words: Array<Record<string, unknown>>;
  memo: string;
}) {
  return {
    id: "transcript-1",
    started_at: 0,
    ended_at: 400,
    memo,
    wordsJson: JSON.stringify(words),
    words,
    speaker_hints: [],
  };
}

function snapshot({
  notes = [],
  transcripts = [],
  sessionId = "session-1",
  title = "Planning",
}: {
  notes?: ReturnType<typeof summary>[];
  transcripts?: ReturnType<typeof transcript>[];
  sessionId?: string;
  title?: string;
} = {}) {
  return {
    sessionId,
    title,
    createdAt: "2026-07-10T09:00:00.000Z",
    event: null,
    eventId: null,
    rawMarkdown: "",
    enhancedNotes: notes,
    transcripts,
    participants: [],
  };
}

function buildTool({
  sessionId = "session-1",
  enhancedNoteId,
}: {
  sessionId?: string;
  enhancedNoteId?: string;
} = {}) {
  return buildApplySessionCorrectionTool({
    getSessionId: () => sessionId,
    getEnhancedNoteId: () => enhancedNoteId,
  });
}

describe("session correction chat tool", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.applySessionContentCorrections.mockResolvedValue(undefined);
    mocks.updateSettingValue.mockImplementation(async (_key, update) =>
      update("[]"),
    );
    // Vorgabe: Rust verwirft nichts. Wer eine Verwerfung pruefen will, setzt
    // sie in seinem eigenen Test.
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("plans exact summary replacements without mutating the snapshot", () => {
    const note = summary("Discussed X roadmap.");

    const plan = sessionCorrectionTestInternals.planSummaryCorrections({
      notes: [note],
      oldText: "X roadmap",
      newText: "Y roadmap",
    });

    expect(plan.changes).toEqual([
      {
        enhancedNoteId: "note-1",
        title: "Summary",
        replacements: 1,
      },
    ]);
    expect(json2md(parseJsonContent(plan.updates[0]?.nextContent))).toContain(
      "Y roadmap",
    );
    expect(note.markdown).toContain("X roadmap");
  });

  it("plans transcript word and memo corrections together", () => {
    const source = transcript({
      words: [
        { id: "w1", text: "It", start_ms: 0, end_ms: 100, channel: 0 },
        { id: "w2", text: "is", start_ms: 100, end_ms: 200, channel: 0 },
        { id: "w3", text: "X", start_ms: 200, end_ms: 300, channel: 0 },
      ],
      memo: "Speaker 1: It is X",
    });

    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [source] as any,
      oldText: "X",
      newText: "Y",
    });

    expect(plan.changes).toEqual([
      {
        transcriptId: "transcript-1",
        wordReplacements: 1,
        memoReplacements: 1,
      },
    ]);
    expect(JSON.parse(plan.updates[0]!.nextWordsJson)).toMatchObject([
      { text: "It" },
      { text: "is" },
      { text: "Y" },
    ]);
    expect(plan.updates[0]?.nextMemo).toBe("Speaker 1: It is Y");
    expect(source.memo).toBe("Speaker 1: It is X");
  });

  it("updates every repeated transcript phrase", () => {
    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [
        transcript({
          words: [
            { id: "w1", text: "X", start_ms: 0, end_ms: 100, channel: 0 },
            { id: "w2", text: "then", start_ms: 100, end_ms: 200, channel: 0 },
            { id: "w3", text: "X", start_ms: 200, end_ms: 300, channel: 0 },
          ],
          memo: "Speaker 1: X then X",
        }),
      ] as any,
      oldText: "X",
      newText: "Y",
    });

    expect(plan.changes[0]).toMatchObject({
      wordReplacements: 2,
      memoReplacements: 2,
    });
    expect(plan.updates[0]?.nextMemo).toBe("Speaker 1: Y then Y");
  });

  it("replaces bounded terms inside compound transcript words", () => {
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        {
          id: "w1",
          text: " mini-Tara,",
          start_ms: 0,
          end_ms: 100,
          channel: 0,
        },
        {
          id: "w2",
          text: "Tara",
          start_ms: 100,
          end_ms: 200,
          channel: 0,
        },
        {
          id: "w3",
          text: "Tarantino",
          start_ms: 200,
          end_ms: 300,
          channel: 0,
        },
      ],
      "Tara",
      "Char",
    );

    expect(result.count).toBe(2);
    expect(result.words).toEqual([
      {
        id: "w1",
        text: " mini-Char,",
        start_ms: 0,
        end_ms: 100,
        channel: 0,
      },
      {
        id: "w2",
        text: "Char",
        start_ms: 100,
        end_ms: 200,
        channel: 0,
      },
      {
        id: "w3",
        text: "Tarantino",
        start_ms: 200,
        end_ms: 300,
        channel: 0,
      },
    ]);
  });

  it("keeps compound transcript words and memo text in sync", () => {
    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [
        transcript({
          words: [
            {
              id: "w1",
              text: "mini-tara",
              start_ms: 0,
              end_ms: 100,
              channel: 0,
            },
            {
              id: "w2",
              text: "Tarantino",
              start_ms: 100,
              end_ms: 200,
              channel: 0,
            },
          ],
          memo: "Speaker 1: mini-tara, not Tarantino",
        }),
      ] as any,
      oldText: "Tara",
      newText: "Char",
    });

    expect(plan.changes[0]).toMatchObject({
      wordReplacements: 1,
      memoReplacements: 1,
    });
    expect(JSON.parse(plan.updates[0]!.nextWordsJson)).toMatchObject([
      { text: "mini-Char" },
      { text: "Tarantino" },
    ]);
    expect(plan.updates[0]?.nextMemo).toBe(
      "Speaker 1: mini-Char, not Tarantino",
    );
  });

  it("falls back to normalized matching for transcript memos", () => {
    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [
        transcript({
          words: [
            {
              id: "w1",
              text: "Ta-ra",
              start_ms: 0,
              end_ms: 100,
              channel: 0,
            },
          ],
          memo: "Speaker 1: Ta-ra",
        }),
      ] as any,
      oldText: "Tara",
      newText: "Char",
    });

    expect(plan.changes[0]).toMatchObject({
      wordReplacements: 1,
      memoReplacements: 1,
    });
    expect(JSON.parse(plan.updates[0]!.nextWordsJson)).toMatchObject([
      { text: "Char" },
    ]);
    expect(plan.updates[0]?.nextMemo).toBe("Speaker 1: Char");
  });

  it("does not plan a partial transcript row update", () => {
    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [
        transcript({
          words: [
            { id: "w1", text: "X", start_ms: 0, end_ms: 100, channel: 0 },
          ],
          memo: "Speaker 1: no correction here",
        }),
      ] as any,
      oldText: "X",
      newText: "Y",
    });

    expect(plan).toEqual({ changes: [], updates: [] });
  });

  it("does not remove transcript words for blank replacement text", () => {
    const plan = sessionCorrectionTestInternals.planTranscriptCorrections({
      transcripts: [
        transcript({
          words: [
            { id: "w1", text: "X", start_ms: 0, end_ms: 100, channel: 0 },
          ],
          memo: "Speaker 1: X",
        }),
      ] as any,
      oldText: "X",
      newText: "   ",
    });

    expect(plan).toEqual({ changes: [], updates: [] });
  });

  it("can replace transcript phrases with a different word count", () => {
    // Fork 02.09.2026: das Beispiel hiess schon immer "different word count",
    // ersetzte aber zwei Woerter durch zwei -- und traf damit seit den
    // gemessenen Wortzeiten den Zweig, der jedem Wort SEINE Zeit laesst. Drei
    // statt zwei Woerter ist der Fall, den der Name meint.
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        { id: "w1", text: "not", start_ms: 0, end_ms: 100, channel: 0 },
        { id: "w2", text: "X", start_ms: 100, end_ms: 300, channel: 0 },
      ],
      "not X",
      "Y instead here",
    );

    expect(result.count).toBe(1);
    expect(result.words).toMatchObject([
      { id: "w1", text: "Y", start_ms: 0, end_ms: 100 },
      { id: "w1:correction:1", text: "instead", start_ms: 100, end_ms: 200 },
      { id: "w1:correction:2", text: "here", start_ms: 200, end_ms: 300 },
    ]);
  });

  it("keeps every measured word time when the word count does not change", () => {
    // Seit die Wortzeiten gemessen sind (Fork 02.09.2026), darf eine
    // Namenskorrektur sie nicht wieder gleichverteilen -- hier ist gar nichts
    // zu verteilen, es wird nur anders geschrieben.
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        {
          id: "w1",
          text: "Bo",
          start_ms: 1000,
          end_ms: 1120,
          channel: 0,
          metadata: { timing: { source: "provider_word" } },
        },
        {
          id: "w2",
          text: "Fridrich",
          start_ms: 1600,
          end_ms: 2000,
          channel: 0,
          metadata: { timing: { source: "provider_word" } },
        },
      ],
      "Bo Fridrich",
      "Bo Friedrich",
    );

    expect(result.count).toBe(1);
    expect(result.words).toMatchObject([
      { id: "w1", text: "Bo", start_ms: 1000, end_ms: 1120 },
      { id: "w2", text: "Friedrich", start_ms: 1600, end_ms: 2000 },
    ]);
    expect(result.words[1].metadata).toEqual({
      timing: { source: "provider_word" },
    });
  });

  it("stops calling the times measured once it has to redistribute them", () => {
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        {
          id: "w1",
          text: "X",
          start_ms: 0,
          end_ms: 400,
          channel: 0,
          metadata: { timing: { source: "provider_word" } },
        },
      ],
      "X",
      "eins zwei",
    );

    expect(result.count).toBe(1);
    expect(result.words).toHaveLength(2);
    for (const word of result.words) {
      expect(word.metadata).toEqual({
        timing: { source: "provider_segment_interpolated" },
      });
    }
  });

  it("nimmt bei anderer Wortzahl die vorsichtigste gemessene Sicherheit", () => {
    // Fork 03.09.2026: bis hierher erbte JEDES neue Wort die Metadaten des
    // ersten -- samt seiner gemessenen Sicherheit. Aus 0,91 und 0,42 wurden
    // dreimal 0,91, und eine erfundene Zahl landete dauerhaft in words_json.
    // Dieselbe Regel wie im Rust-Nachlauf: das Minimum der Gruppe.
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        {
          id: "w1",
          text: "nicht",
          start_ms: 0,
          end_ms: 100,
          channel: 0,
          metadata: { timing: { source: "provider_word" }, confidence: 0.91 },
        },
        {
          id: "w2",
          text: "X",
          start_ms: 100,
          end_ms: 300,
          channel: 0,
          metadata: { timing: { source: "provider_word" }, confidence: 0.42 },
        },
      ],
      "nicht X",
      "eins zwei drei",
    );

    expect(result.count).toBe(1);
    expect(result.words).toHaveLength(3);
    for (const word of result.words) {
      expect(word.metadata).toEqual({
        timing: { source: "provider_segment_interpolated" },
        confidence: 0.42,
      });
    }
  });

  it("laesst die gemessene Sicherheit ganz weg, wenn einer der Gruppe keine hat", () => {
    // Das Minimum der uebrigen waere eine Aussage ueber eine Teilmenge, die
    // sich als Aussage ueber das ganze ersetzte Wort liest. Dann lieber nichts.
    const result = sessionCorrectionTestInternals.replaceTranscriptWords(
      [
        {
          id: "w1",
          text: "nicht",
          start_ms: 0,
          end_ms: 100,
          channel: 0,
          metadata: { timing: { source: "provider_word" }, confidence: 0.91 },
        },
        {
          id: "w2",
          text: "X",
          start_ms: 100,
          end_ms: 300,
          channel: 0,
          metadata: { timing: { source: "provider_word" } },
        },
      ],
      "nicht X",
      "eins zwei drei",
    );

    expect(result.count).toBe(1);
    expect(result.words).toHaveLength(3);
    for (const word of result.words) {
      expect(word.metadata).toEqual({
        timing: { source: "provider_segment_interpolated" },
      });
    }
  });

  it("commits summary and transcript corrections before dictionary terms", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({
        notes: [
          summary("Sam (from Airborne Brothers) liked the OpenWorld concept."),
        ],
        transcripts: [
          transcript({
            words: [
              { id: "w1", text: "sam", start_ms: 0, end_ms: 100, channel: 0 },
              {
                id: "w2",
                text: "from",
                start_ms: 100,
                end_ms: 200,
                channel: 0,
              },
              {
                id: "w3",
                text: "Airborne",
                start_ms: 200,
                end_ms: 300,
                channel: 0,
              },
              {
                id: "w4",
                text: "Brothers,",
                start_ms: 300,
                end_ms: 400,
                channel: 0,
              },
            ],
            memo: "Speaker 1: sam from Airborne Brothers, liked it.",
          }),
        ],
      }),
    );
    let persistedDictionary = "";
    mocks.updateSettingValue.mockImplementation(async (_key, update) => {
      persistedDictionary = update(JSON.stringify(["Anarlog"]));
      return persistedDictionary;
    });

    const result = await (
      buildTool({ enhancedNoteId: "note-1" }) as any
    ).execute({
      oldText: "Sam (from Airborne Brothers)",
      newText: "Tim from Erebor",
      dictionaryTerms: ["Erebor"],
    });

    expect(result).toMatchObject({
      status: "applied",
      summaryChanges: [{ enhancedNoteId: "note-1", replacements: 1 }],
      transcriptChanges: [
        {
          transcriptId: "transcript-1",
          wordReplacements: 1,
          memoReplacements: 1,
        },
      ],
      titleChange: null,
      dictionaryChanges: { addedTerms: ["Erebor"] },
    });
    expect(mocks.applySessionContentCorrections).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "session-1",
        summaries: [expect.objectContaining({ id: "note-1" })],
        transcripts: [expect.objectContaining({ id: "transcript-1" })],
      }),
    );
    expect(
      mocks.applySessionContentCorrections.mock.invocationCallOrder[0],
    ).toBeLessThan(mocks.updateSettingValue.mock.invocationCallOrder[0]);
    expect(persistedDictionary).toBe(JSON.stringify(["Anarlog", "Erebor"]));
  });

  it("updates the visible session title even when it is not in the summary body", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({
        title: "Scratchpad Design and Analog vs Chyle Direction",
        notes: [summary("Pinning stays on the permanent page.")],
      }),
    );

    const result = await (buildTool() as any).execute({
      target: "summary",
      oldText: "Analog",
      newText: "Anarlog",
    });

    expect(result).toMatchObject({
      status: "applied",
      summaryChanges: [],
      transcriptChanges: [],
      titleChange: {
        replacements: 1,
        nextTitle: "Scratchpad Design and Anarlog vs Chyle Direction",
      },
    });
    expect(mocks.applySessionContentCorrections).toHaveBeenCalledWith({
      sessionId: "session-1",
      summaries: [],
      transcripts: [],
      title: {
        currentTitle: "Scratchpad Design and Analog vs Chyle Direction",
        nextTitle: "Scratchpad Design and Anarlog vs Chyle Direction",
      },
    });
  });

  it("reports partial success when a requested target does not match", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({ notes: [summary("Discussed X roadmap.")] }),
    );

    const result = await (buildTool() as any).execute({
      oldText: "X roadmap",
      newText: "Y roadmap",
    });

    expect(result).toMatchObject({
      status: "partial",
      message:
        "Applied correction where matched, but no matching transcript text was found.",
      summaryChanges: [{ enhancedNoteId: "note-1", replacements: 1 }],
      transcriptChanges: [],
    });
  });

  it("returns an explicit error for a summary id outside the session", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({ notes: [summary("Discussed X roadmap.")] }),
    );

    const result = await (buildTool() as any).execute({
      target: "summary",
      enhancedNoteId: "missing-note",
      oldText: "X roadmap",
      newText: "Y roadmap",
    });

    expect(result).toEqual({
      status: "error",
      message: "The requested summary does not belong to the target session.",
      sessionId: "session-1",
    });
    expect(mocks.applySessionContentCorrections).not.toHaveBeenCalled();
  });

  it("still corrects the transcript when the default target has an invalid summary id", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({
        notes: [summary("No correction here.")],
        transcripts: [
          transcript({
            words: [
              { id: "w1", text: "X", start_ms: 0, end_ms: 100, channel: 0 },
            ],
            memo: "Speaker 1: X",
          }),
        ],
      }),
    );

    const result = await (buildTool() as any).execute({
      enhancedNoteId: "missing-note",
      oldText: "X",
      newText: "Y",
    });

    expect(result).toMatchObject({
      status: "applied",
      summaryChanges: [],
      transcriptChanges: [{ transcriptId: "transcript-1" }],
    });
  });

  it("defaults summary correction to the active enhanced note", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({
        notes: [
          summary("Discussed X roadmap.", "note-1", "Summary"),
          summary("Discussed X roadmap.", "note-2", "Other"),
        ],
      }),
    );

    const result = await (
      buildTool({ enhancedNoteId: "note-1" }) as any
    ).execute({
      target: "summary",
      oldText: "X roadmap",
      newText: "Y roadmap",
    });

    expect(result.summaryChanges).toEqual([
      {
        enhancedNoteId: "note-1",
        title: "Summary",
        replacements: 1,
      },
    ]);
    expect(
      mocks.applySessionContentCorrections.mock.calls[0][0].summaries,
    ).toEqual([expect.objectContaining({ id: "note-1" })]);
  });

  it("does not use the active summary for an explicit session", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({
        sessionId: "session-2",
        notes: [summary("Discussed X roadmap.", "note-2", "Target")],
      }),
    );

    const result = await (
      buildTool({ enhancedNoteId: "note-1" }) as any
    ).execute({
      sessionId: "session-2",
      target: "summary",
      oldText: "X roadmap",
      newText: "Y roadmap",
    });

    expect(result).toMatchObject({
      status: "applied",
      sessionId: "session-2",
      summaryChanges: [{ enhancedNoteId: "note-2", title: "Target" }],
    });
  });

  it("reports a stale transaction instead of saving dictionary terms", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({ notes: [summary("Discussed X roadmap.")] }),
    );
    mocks.applySessionContentCorrections.mockRejectedValueOnce(
      new Error("expected 1 row"),
    );

    const result = await (buildTool() as any).execute({
      target: "summary",
      oldText: "X roadmap",
      newText: "Y roadmap",
      dictionaryTerms: ["Y"],
    });

    expect(result).toEqual({
      status: "error",
      message:
        "The note changed before the correction could be committed. Read the note and retry.",
      sessionId: "session-1",
    });
    expect(mocks.updateSettingValue).not.toHaveBeenCalled();
  });

  it("does not report a durable correction as failed when dictionary storage fails", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({ notes: [summary("Discussed X roadmap.")] }),
    );
    mocks.updateSettingValue.mockRejectedValueOnce(new Error("settings busy"));

    const result = await (buildTool() as any).execute({
      target: "summary",
      oldText: "X roadmap",
      newText: "Y roadmap",
      dictionaryTerms: ["Y roadmap"],
    });

    expect(result).toMatchObject({
      status: "applied",
      message:
        "The correction was applied, but dictionary terms could not be saved.",
      summaryChanges: [{ enhancedNoteId: "note-1" }],
      dictionaryChanges: { addedTerms: [] },
    });
  });
});

describe("Korrekturen lernen die Verhoerung", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.applySessionContentCorrections.mockResolvedValue(undefined);
    // Ausdruecklich hier, nicht geerbt: sonst haengt dieser Block an der
    // Reihenfolge der Bloecke darueber.
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  async function correct({
    stored,
    oldText,
    newText,
    dictionaryTerms,
  }: {
    stored: string[];
    oldText: string;
    newText: string;
    dictionaryTerms: string[];
  }) {
    mocks.loadSessionContentSnapshot.mockResolvedValue(
      snapshot({ notes: [summary(`Termin mit ${oldText} war gut.`)] }),
    );
    let persisted = "";
    mocks.updateSettingValue.mockImplementation(async (_key, update) => {
      persisted = update(JSON.stringify(stored));
      return persisted;
    });

    const result = await (
      buildTool({ enhancedNoteId: "note-1" }) as any
    ).execute({
      target: "summary",
      oldText,
      newText,
      dictionaryTerms,
    });

    return { persisted: JSON.parse(persisted || "[]") as string[], result };
  }

  it("speichert das Paar als Alias, nicht nur die richtige Schreibweise", async () => {
    const { persisted } = await correct({
      stored: [],
      oldText: "Sarec",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz"],
    });

    expect(persisted).toEqual(["Sedacz => Sarec"]);
  });

  it("haengt an den vorhandenen Eintrag an statt eine Dublette anzulegen", async () => {
    const { persisted } = await correct({
      stored: ["Nordwerk", "Sedacz => Sarec"],
      oldText: "Saredi",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz"],
    });

    expect(persisted).toEqual(["Nordwerk", "Sedacz => Sarec; Saredi"]);
    expect(
      persisted.filter((term) => term.startsWith("Sedacz")),
    ).toHaveLength(1);
  });

  it("macht aus einer Satzkorrektur keinen Alias", async () => {
    const { persisted } = await correct({
      stored: [],
      oldText: "das haben wir letzte Woche schon besprochen",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz"],
    });

    expect(persisted).toEqual(["Sedacz"]);
  });

  it("haengt die Verhoerung nur an den Begriff, der wirklich ersetzt wurde", async () => {
    const { persisted } = await correct({
      stored: [],
      oldText: "Sarec",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz", "Nordwerk"],
    });

    expect(persisted).toEqual(["Sedacz => Sarec", "Nordwerk"]);
  });

  it("meldet den gelernten Alias zurueck", async () => {
    const { result } = await correct({
      stored: [],
      oldText: "Sarec",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz"],
    });

    expect(result.dictionaryChanges).toMatchObject({
      addedTerms: ["Sedacz"],
      learnedAliases: [{ canonical: "Sedacz", alias: "Sarec" }],
    });
  });

  it("lernt den Alias auch, wenn der Begriff schon im Woerterbuch steht", async () => {
    // Der Regelfall: die Liste des Betreibers enthaelt alle 20 Namen laengst, also hat
    // das Modell keinen Anlass, "Sedacz" nochmal als neuen Begriff zu
    // schicken. Wer nur auf dictionaryTerms schaut, lernt hier nichts.
    const { persisted } = await correct({
      stored: ["Sedacz", "Nordwerk"],
      oldText: "Sarec",
      newText: "Sedacz",
      dictionaryTerms: [],
    });

    expect(persisted).toEqual(["Sedacz => Sarec", "Nordwerk"]);
  });

  it("meldet bei der zweiten, gleichen Korrektur nichts frisch Gelerntes", async () => {
    // Sonst erzaehlt das Modell dem Menschen "habe ich mir gemerkt", obwohl
    // sich an der Liste nichts geaendert hat.
    const { persisted, result } = await correct({
      stored: ["Sedacz => Sarec"],
      oldText: "Sarec",
      newText: "Sedacz",
      dictionaryTerms: ["Sedacz"],
    });

    expect(persisted).toEqual(["Sedacz => Sarec"]);
    expect(result.dictionaryChanges).toMatchObject({
      addedTerms: [],
      learnedAliases: [],
    });
  });

  it("fasst die Einstellung gar nicht an, wenn es nichts zu lernen gibt", async () => {
    // Kein Begriff, und das Paar ist eine Satzkorrektur. Ein Schreibvorgang
    // waere hier keine Aenderung, sondern nur ein Risiko.
    await correct({
      stored: ["Sedacz"],
      oldText: "das haben wir letzte Woche schon besprochen",
      newText: "das haben wir gestern besprochen",
      dictionaryTerms: [],
    });

    expect(mocks.updateSettingValue).not.toHaveBeenCalled();
  });
});

describe("Was eine Korrektur dem Woerterbuch beibringen darf (03.09.2026)", () => {
  function store(initial: string[]) {
    let value = JSON.stringify(initial);
    mocks.updateSettingValue.mockImplementation(async (_key, update) => {
      value = update(value);
    });
    return () => JSON.parse(value) as string[];
  }

  beforeEach(() => {
    mocks.updateSettingValue.mockReset();
    mocks.vocabularyRiskyAliases.mockReset();
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });
  });

  // Der teuerste Fall: eine reine Sachkorrektur wird zur Dauerregel. "Montag"
  // -> "Dienstag" ist formal ein Ein-Wort-Paar; bis hierher legte der Rueckfall
  // auf `newText` daraus ungefragt "Dienstag => Montag" an.
  it("macht aus einer Sachkorrektur keinen Woerterbuch-Eintrag", async () => {
    const gespeichert = store([]);

    const result = await sessionCorrectionTestInternals.saveDictionaryTerms(
      undefined,
      { oldText: "Montag", newText: "Dienstag" },
    );

    expect(gespeichert()).toEqual([]);
    expect(result.learnedAliases).toEqual([]);
  });

  it("lernt weiter, wenn der Name schon in der Liste steht", async () => {
    const gespeichert = store(["Sedacz"]);

    const result = await sessionCorrectionTestInternals.saveDictionaryTerms(
      undefined,
      { oldText: "Sarec", newText: "Sedacz" },
    );

    expect(gespeichert()).toEqual(["Sedacz => Sarec"]);
    expect(result.learnedAliases).toEqual([
      { canonical: "Sedacz", alias: "Sarec" },
    ]);
  });

  it("lernt weiter, wenn das Modell den Namen in diesem Aufruf mitschickt", async () => {
    const gespeichert = store([]);

    const result = await sessionCorrectionTestInternals.saveDictionaryTerms(
      ["Sedacz"],
      { oldText: "Sarec", newText: "Sedacz" },
    );

    expect(gespeichert()).toEqual(["Sedacz => Sarec"]);
    expect(result.learnedAliases).toEqual([
      { canonical: "Sedacz", alias: "Sarec" },
    ]);
  });

  // "Kling" ist gewoehnliches Deutsch. `is_all_common_german_phrase`
  // (crates/vocabulary/src/matcher.rs) wirft die Verhoerung weg -- der Eintrag
  // wuerde also nie wirken. Ihn trotzdem als gelernt zu melden ist die Luege,
  // um die es hier geht.
  it("meldet eine gewoehnlich-deutsche Verhoerung als verworfen, nie als gelernt", async () => {
    const gespeichert = store(["Glinck"]);
    mocks.vocabularyRiskyAliases.mockResolvedValue({
      status: "ok",
      data: [{ canonical: "Glinck", alias: "Kling" }],
    });

    const result = await sessionCorrectionTestInternals.saveDictionaryTerms(
      undefined,
      { oldText: "Kling", newText: "Glinck" },
    );

    expect(result.learnedAliases).toEqual([]);
    expect(result.ignoredAliases).toEqual([
      { canonical: "Glinck", alias: "Kling" },
    ]);
    expect(gespeichert()).toEqual(["Glinck"]);
    // Die Zeile, die Rust vorgelegt bekam, ist die, um die es geht.
    expect(mocks.vocabularyRiskyAliases).toHaveBeenCalledWith([
      "Glinck => Kling",
    ]);
  });

  it("lernt, was Rust NICHT verwirft -- auch wenn es deutsch aussieht", async () => {
    const gespeichert = store(["Glinck"]);
    mocks.vocabularyRiskyAliases.mockResolvedValue({ status: "ok", data: [] });

    const result = await sessionCorrectionTestInternals.saveDictionaryTerms(
      undefined,
      { oldText: "Kling", newText: "Glinck" },
    );

    expect(result.ignoredAliases).toEqual([]);
    expect(gespeichert()).toEqual(["Glinck => Kling"]);
  });
});
