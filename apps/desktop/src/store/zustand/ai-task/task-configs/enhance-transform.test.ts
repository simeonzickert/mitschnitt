import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { enhanceTransform } from "./enhance-transform";

const mocks = vi.hoisted(() => ({
  collectEnhanceImageContext: vi.fn(),
  getTemplateById: vi.fn(),
  formatMeetingChatContext: vi.fn(),
  loadMeetingChatRecords: vi.fn(),
  loadSessionContentSnapshot: vi.fn(),
  loadHumansByIds: vi.fn(),
  buildRenderTranscriptRequestFromRows: vi.fn(),
  collectAssignedHumanIdsFromTranscriptRows: vi.fn(),
  renderTranscriptSegments: vi.fn(),
}));

vi.mock("./enhance-images", () => ({
  collectEnhanceImageContext: mocks.collectEnhanceImageContext,
}));

vi.mock("~/templates/queries", () => ({
  getTemplateById: mocks.getTemplateById,
}));

vi.mock("~/session/content-queries", () => ({
  loadSessionContentSnapshot: mocks.loadSessionContentSnapshot,
}));

vi.mock("~/stt/meeting-chat-records", () => ({
  formatMeetingChatContext: mocks.formatMeetingChatContext,
  loadMeetingChatRecords: mocks.loadMeetingChatRecords,
}));

vi.mock("~/contacts/queries", () => ({
  loadHumansByIds: mocks.loadHumansByIds,
}));

vi.mock("~/stt/render-transcript", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("~/stt/render-transcript")>();
  return {
    buildRenderTranscriptRequestFromRows:
      mocks.buildRenderTranscriptRequestFromRows,
    collectAssignedHumanIdsFromTranscriptRows:
      mocks.collectAssignedHumanIdsFromTranscriptRows,
    renderTranscriptSegments: mocks.renderTranscriptSegments,
    // Bewusst NICHT gemockt: reine Rechnung ohne Seiteneffekt.
    formatSegmentStartLabel: actual.formatSegmentStartLabel,
  };
});

function createSnapshot() {
  return {
    sessionId: "session-1",
    ownerUserId: "user-1",
    title: "Weekly Review",
    createdAt: "2026-07-10T00:00:00.000Z",
    event: null,
    sourceApps: [],
    eventId: null,
    rawNoteId: "session-1",
    rawTemplateId: "",
    rawContent: "![post](asset://localhost/post.png)",
    rawContentFormat: "markdown",
    rawMarkdown: "![post](asset://localhost/post.png)",
    enhancedNotes: [],
    transcripts: [
      {
        id: "transcript-1",
        started_at: 100,
        ended_at: 200,
        memo: "![pre](asset://localhost/pre.png)",
        wordsJson: "[]",
        words: [],
        speaker_hints: [],
      },
    ],
    participants: [{ humanId: "human-1", name: "Alice", jobTitle: "Engineer" }],
  };
}

const settingsValues = { ai_language: "en" } as const;

describe("enhanceTransform.transformArgs", () => {
  let consoleError: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.collectEnhanceImageContext.mockResolvedValue([]);
    mocks.getTemplateById.mockResolvedValue(null);
    mocks.formatMeetingChatContext.mockReturnValue("");
    mocks.loadMeetingChatRecords.mockResolvedValue([]);
    mocks.loadSessionContentSnapshot.mockResolvedValue(createSnapshot());
    mocks.loadHumansByIds.mockResolvedValue([{ id: "human-1", name: "Alice" }]);
    mocks.collectAssignedHumanIdsFromTranscriptRows.mockReturnValue([]);
    mocks.buildRenderTranscriptRequestFromRows.mockReturnValue(null);
    mocks.renderTranscriptSegments.mockResolvedValue([]);
    consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    consoleError.mockRestore();
  });

  it("uses the selected template when it can be loaded", async () => {
    mocks.getTemplateById.mockResolvedValue({
      title: "Standup",
      description: "Daily sync",
      sections: [{ title: "Updates", description: null }],
    });

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
        templateId: "template-1",
      },
      settingsValues,
    );

    expect(result.template).toEqual({
      title: "Standup",
      description: "Daily sync",
      sections: [{ title: "Updates", description: null }],
    });
    expect(result.participants).toEqual([
      { name: "Alice", jobTitle: "Engineer" },
    ]);
  });

  it("gibt jedem Block seine Startzeit mit in den Prompt", async () => {
    // Seit die Block-Bildung an den letzten Block DESSELBEN Kanals anhaengt,
    // ueberlappen sich Bloecke in der Zeit. Ohne Zeitmarke liest das
    // Sprachmodell zwei Monologe statt eines Gespraechs.
    mocks.buildRenderTranscriptRequestFromRows.mockReturnValue({
      transcripts: [],
      participant_human_ids: [],
      self_human_id: null,
      humans: [],
    });
    mocks.renderTranscriptSegments.mockResolvedValue([
      {
        speaker_label: "Alice",
        text: "Wir fangen an",
        start_ms: 12_000,
        end_ms: 15_000,
        words: [{ text: "Wir", start_ms: 12_000, end_ms: 12_400 }],
      },
      {
        speaker_label: "Bob",
        text: "Ja",
        start_ms: 3_725_000,
        end_ms: 3_726_000,
        words: [{ text: "Ja", start_ms: 3_725_000, end_ms: 3_726_000 }],
      },
    ]);

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
      },
      settingsValues,
    );

    expect(result.transcripts[0]?.segments).toEqual([
      { speaker: "Alice", text: "Wir fangen an", startLabel: "00:12" },
      { speaker: "Bob", text: "Ja", startLabel: "1:02:05" },
    ]);
  });

  describe("ZICK-312: Namen nur bei echter Sprechertrennung", () => {
    const renderedSegments = [
      {
        speaker_label: "Alex",
        text: "Das machen wir so",
        start_ms: 1_000,
        end_ms: 2_000,
        words: [{ text: "Das", start_ms: 1_000, end_ms: 1_400 }],
      },
      {
        speaker_label: "Alex",
        text: "Ich schicke das Montag",
        start_ms: 3_000,
        end_ms: 4_000,
        words: [{ text: "Ich", start_ms: 3_000, end_ms: 3_400 }],
      },
    ];

    function snapshotWith(
      words: Array<{ id: string; channel?: number }>,
      speakerHints: Array<{
        id: string;
        word_id: string;
        type: string;
        value: string;
      }> = [],
    ) {
      const snapshot = createSnapshot();
      snapshot.transcripts[0] = {
        ...snapshot.transcripts[0]!,
        words: words.map((word) => ({
          ...word,
          text: "x",
          start_ms: 0,
          end_ms: 1,
        })) as never,
        speaker_hints: speakerHints as never,
      };
      return snapshot;
    }

    beforeEach(() => {
      mocks.buildRenderTranscriptRequestFromRows.mockReturnValue({
        transcripts: [],
        participant_human_ids: [],
        self_human_id: "user-1",
        humans: [],
      });
      mocks.renderTranscriptSegments.mockResolvedValue(renderedSegments);
    });

    it("nimmt ohne Trennung jeden Namen aus den Zeilen", async () => {
      // Cloud-Transkript: alle Woerter auf Kanal 0, kein Sprecherhinweis. Die
      // Anzeige etikettiert das als den Nutzer selbst; im Prompt waere das
      // eine Zuschreibung, die es nicht gibt.
      mocks.loadSessionContentSnapshot.mockResolvedValue(
        snapshotWith([
          { id: "w1", channel: 0 },
          { id: "w2", channel: 0 },
        ]),
      );

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      const speakers = result.transcripts[0]?.segments.map((s) => s.speaker);
      expect(speakers).toEqual(["Unknown speaker", "Unknown speaker"]);
      expect(speakers).not.toContain("Alex");
    });

    it("behaelt die Namen, wenn Mikrofon und Systemton getrennt sind", async () => {
      mocks.loadSessionContentSnapshot.mockResolvedValue(
        snapshotWith([
          { id: "w1", channel: 0 },
          { id: "w2", channel: 1 },
        ]),
      );

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      expect(result.transcripts[0]?.segments.map((s) => s.speaker)).toEqual([
        "Alex",
        "Alex",
      ]);
    });

    it("behaelt die Namen, wenn der Anbieter mehrere Sprecher getrennt hat", async () => {
      mocks.loadSessionContentSnapshot.mockResolvedValue(
        snapshotWith(
          [
            { id: "w1", channel: 0 },
            { id: "w2", channel: 0 },
          ],
          [
            {
              id: "h1",
              word_id: "w1",
              type: "provider_speaker_index",
              value: JSON.stringify({ channel: 0, speaker_index: 0 }),
            },
            {
              id: "h2",
              word_id: "w2",
              type: "provider_speaker_index",
              value: JSON.stringify({ channel: 0, speaker_index: 1 }),
            },
          ],
        ),
      );

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      expect(result.transcripts[0]?.segments[0]?.speaker).toBe("Alex");
    });

    it("benennt nur den Block, den ein Mensch von Hand zugeordnet hat", async () => {
      // Frueher schaltete eine einzige Handzuordnung ALLE Namen wieder ein.
      mocks.renderTranscriptSegments.mockResolvedValue([
        {
          ...renderedSegments[0]!,
          words: [{ id: "w1", text: "Das", start_ms: 1_000, end_ms: 1_400 }],
        },
        {
          ...renderedSegments[1]!,
          speaker_label: "Max",
          words: [{ id: "w2", text: "Ich", start_ms: 3_000, end_ms: 3_400 }],
        },
      ]);
      mocks.loadSessionContentSnapshot.mockResolvedValue(
        snapshotWith(
          [
            { id: "w1", channel: 0 },
            { id: "w2", channel: 0 },
          ],
          [
            {
              id: "w2:user_speaker_assignment:segment",
              word_id: "w2",
              type: "user_speaker_assignment",
              value: JSON.stringify({
                human_id: "human-max",
                scope: "segment",
                word_ids: ["w2"],
              }),
            },
          ],
        ),
      );

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      expect(result.transcripts[0]?.segments.map((s) => s.speaker)).toEqual([
        "Unknown speaker",
        "Max",
      ]);
    });

    it("nimmt Woertern ohne Kanalangabe den Namen (fail-closed, Forge M5)", async () => {
      mocks.loadSessionContentSnapshot.mockResolvedValue(
        snapshotWith([{ id: "w1" }, { id: "w2" }]),
      );

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      expect(result.transcripts[0]?.segments.map((s) => s.speaker)).toEqual([
        "Unknown speaker",
        "Unknown speaker",
      ]);
    });

    it("laesst ein Selbstgespraech ohne andere Teilnehmer beim Nutzer", async () => {
      const snapshot = snapshotWith([{ id: "w1", channel: 0 }]);
      snapshot.participants = [];
      mocks.loadSessionContentSnapshot.mockResolvedValue(snapshot);

      const result = await enhanceTransform.transformArgs(
        { sessionId: "session-1", enhancedNoteId: "note-1" },
        settingsValues,
      );

      expect(result.transcripts[0]?.segments[0]?.speaker).toBe("Alex");
    });
  });

  it("includes the detected meeting platform in generated context", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue({
      ...createSnapshot(),
      sourceApps: [
        {
          app: "chrome",
          name: "Google Chrome",
          platform: "Google Meet",
        },
      ],
    });

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
      },
      settingsValues,
    );

    expect(result.postMeetingMemo).toContain("Meeting platform: Google Meet");
  });

  it("uses the edited memo headings for its applied template", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue({
      ...createSnapshot(),
      rawTemplateId: "template-1",
      rawContent: JSON.stringify({
        type: "doc",
        content: [
          {
            type: "heading",
            attrs: { level: 2 },
            content: [{ type: "text", text: "Updates" }],
          },
          { type: "paragraph" },
          {
            type: "heading",
            attrs: { level: 2 },
            content: [{ type: "text", text: "Next Steps" }],
          },
        ],
      }),
      rawContentFormat: "prosemirror_json",
      rawMarkdown: "## Updates\n\n## Next Steps",
    });
    mocks.getTemplateById.mockResolvedValue({
      title: "1:1 Meeting",
      description: "Weekly conversation",
      sections: [
        { title: "Updates", description: "Recent changes" },
        { title: "Action Items", description: "Follow-ups" },
      ],
    });

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
        templateId: "template-1",
      },
      settingsValues,
    );

    expect(result.template).toEqual({
      title: "1:1 Meeting",
      description: "Weekly conversation",
      sections: [
        { title: "Updates", description: "Recent changes" },
        { title: "Next Steps", description: "Follow-ups" },
      ],
    });
  });

  it("keeps the applied template when the memo has no section headings", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue({
      ...createSnapshot(),
      rawTemplateId: "template-1",
      rawContent: JSON.stringify({
        type: "doc",
        content: [{ type: "paragraph" }],
      }),
      rawContentFormat: "prosemirror_json",
      rawMarkdown: "Notes without headings",
    });
    mocks.getTemplateById.mockResolvedValue({
      title: "1:1 Meeting",
      description: "Weekly conversation",
      sections: [
        { title: "Updates", description: "Recent changes" },
        { title: "Action Items", description: "Follow-ups" },
      ],
    });

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
        templateId: "template-1",
      },
      settingsValues,
    );

    expect(result.template).toEqual({
      title: "1:1 Meeting",
      description: "Weekly conversation",
      sections: [
        { title: "Updates", description: "Recent changes" },
        { title: "Action Items", description: "Follow-ups" },
      ],
    });
  });

  it("uses the saved format override for Auto summaries", async () => {
    const result = await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      {
        ...settingsValues,
        auto_summary_prompt: "  Start with decisions.  ",
      },
    );

    expect(result.formatOverride).toBe("  Start with decisions.  ");
  });

  it("ignores the Auto override when a named template is selected", async () => {
    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
        templateId: "template-1",
      },
      {
        ...settingsValues,
        auto_summary_prompt: "Start with decisions.",
      },
    );

    expect(result.formatOverride).toBe("");
  });

  it("uses the built-in Auto format when no override is saved", async () => {
    const result = await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      settingsValues,
    );

    expect(result.formatOverride).toBe("");
    expect(result.summaryLength).toBe("detailed");
  });

  it("uses the saved summary length mode", async () => {
    const result = await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      { ...settingsValues, summary_length: "crisp" },
    );

    expect(result.summaryLength).toBe("crisp");
  });

  it("includes personalization dictionary terms for summary spelling", async () => {
    const result = await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      {
        ...settingsValues,
        personalization_dictionary_terms: JSON.stringify(["Anarlog", "Char"]),
      },
    );

    expect(result.dictionaryTerms).toEqual(["Anarlog", "Char"]);
  });

  it("falls back to generic enhancement when template loading fails", async () => {
    mocks.getTemplateById.mockRejectedValue(new Error("Failed query"));

    const result = await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
        templateId: "template-1",
      },
      settingsValues,
    );

    expect(result.template).toBeNull();
    expect(result.formatOverride).toBe("");
    expect(result.session.title).toBe("Weekly Review");
    expect(consoleError).toHaveBeenCalledWith(
      "[enhance] failed to load template",
      expect.any(Error),
    );
  });

  it("collects image context from canonical transcript and note content", async () => {
    await enhanceTransform.transformArgs(
      {
        sessionId: "session-1",
        enhancedNoteId: "note-1",
      },
      {
        current_llm_provider: "openai",
        current_llm_model: "gpt-4o",
        ai_language: "en",
      },
    );

    expect(mocks.collectEnhanceImageContext).toHaveBeenCalledWith("session-1", [
      "![pre](asset://localhost/pre.png)",
      "![post](asset://localhost/post.png)",
    ]);
  });

  it("builds speaker identity context from SQLite humans", async () => {
    mocks.collectAssignedHumanIdsFromTranscriptRows.mockReturnValue([
      "human-2",
    ]);

    await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      settingsValues,
    );

    expect(mocks.loadHumansByIds).toHaveBeenCalledWith([
      "user-1",
      "human-1",
      "human-2",
    ]);
    expect(mocks.buildRenderTranscriptRequestFromRows).toHaveBeenCalledWith(
      expect.any(Array),
      {
        selfHumanId: "user-1",
        humans: [{ human_id: "human-1", name: "Alice" }],
      },
      ["human-1"],
    );
  });

  it("includes captured meeting chat in the post-meeting memo", async () => {
    mocks.loadMeetingChatRecords.mockResolvedValue([
      { text: "Review the rollout plan" },
    ]);
    mocks.formatMeetingChatContext.mockReturnValue(
      "## Meeting chat\n- Slack · Ada\n  Review the rollout plan",
    );

    const result = await enhanceTransform.transformArgs(
      { sessionId: "session-1", enhancedNoteId: "note-1" },
      settingsValues,
    );

    expect(result.postMeetingMemo).toBe(
      "![post](asset://localhost/post.png)\n\n## Meeting chat\n- Slack · Ada\n  Review the rollout plan",
    );
  });

  it("rejects generation when the session no longer exists", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(null);

    await expect(
      enhanceTransform.transformArgs(
        { sessionId: "missing", enhancedNoteId: "note-1" },
        settingsValues,
      ),
    ).rejects.toThrow("Session missing no longer exists");
  });
});
