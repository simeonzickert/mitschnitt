import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  loadSessionContentSnapshot: vi.fn(),
}));

vi.mock("~/session/content-queries", () => ({
  loadSessionContentSnapshot: mocks.loadSessionContentSnapshot,
}));

import { titleTransform } from "./title-transform";

import type { SettingValues } from "~/settings/schema";

const settings = {
  ai_language: "de",
  personalization_dictionary_terms: JSON.stringify(["Sedacz"]),
} as unknown as SettingValues;

function snapshot() {
  return {
    title: "Jourfixe",
    event: {
      tracking_id: "t-1",
      calendar_id: "c-1",
      title: "Sedacz Jourfixe",
      started_at: "2026-09-02T10:00:00Z",
      ended_at: "2026-09-02T10:45:00Z",
      is_all_day: false,
      has_recurrence_rules: false,
    },
    participants: [
      { name: "Sarec", jobTitle: "Projektleitung" },
      { name: "", jobTitle: null },
    ],
    enhancedNotes: [{ markdown: "Serredi berichtet vom Stand." }],
    rawMarkdown: "",
    transcripts: [],
    sourceApps: [],
  };
}

describe("Der Titel-Prompt bekommt den Kontext der Sitzung", () => {
  beforeEach(() => {
    mocks.loadSessionContentSnapshot.mockReset();
  });

  // Der Befund vom 03.09.2026: der Titel-Pfad hatte weder Kalendertitel noch
  // Teilnehmer -- nur die fertige Notiz. Aus dem verhoerten "Sarec" konnte das
  // Modell deshalb ungestraft "Serredi" bauen.
  it("reicht Kalendertitel, Zeitraum und Teilnehmer weiter", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(snapshot());

    const args = await titleTransform.transformArgs(
      { sessionId: "session-1" },
      settings,
    );

    expect(args.session).toEqual({
      title: "Sedacz Jourfixe",
      startedAt: "2026-09-02T10:00:00Z",
      endedAt: "2026-09-02T10:45:00Z",
      event: { name: "Sedacz Jourfixe" },
    });
    expect(args.participants).toEqual([
      { name: "Sarec", jobTitle: "Projektleitung" },
    ]);
  });

  // Auch dann, wenn die Notiz schon mitgeliefert wurde -- vorher wurde der
  // Schnappschuss in genau diesem Fall gar nicht erst geladen.
  it("holt den Kontext auch bei mitgelieferter Notiz", async () => {
    mocks.loadSessionContentSnapshot.mockResolvedValue(snapshot());

    const args = await titleTransform.transformArgs(
      { sessionId: "session-1", enhancedNote: "Fertige Notiz." },
      settings,
    );

    expect(args.enhancedNote).toBe("Fertige Notiz.");
    expect(args.participants).toEqual([
      { name: "Sarec", jobTitle: "Projektleitung" },
    ]);
  });
});
