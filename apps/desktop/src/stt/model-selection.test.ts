import { describe, expect, test } from "vitest";

import { isSupportedLocalSttModel } from "./capabilities";
import {
  normalizeStoredSttModel,
  normalizeStoredSttSelection,
} from "./model-selection";

describe("normalizeStoredSttSelection", () => {
  test("leaves an untouched selection alone", () => {
    expect(
      normalizeStoredSttSelection("soniqo", "soniqo-parakeet-batch"),
    ).toEqual({
      provider: "soniqo",
      model: "soniqo-parakeet-batch",
    });
  });

  test("moves a stored Soniqo model off the old Anarlog provider", () => {
    expect(
      normalizeStoredSttSelection("anarlog", "soniqo-parakeet-batch"),
    ).toEqual({
      provider: "soniqo",
      model: "soniqo-parakeet-batch",
    });
  });

  // Die drei Argmax-Modelle sind am 01.09.2026 entfallen. Wer sie gespeichert hatte,
  // muss auf einem existierenden Modell landen -- sonst schickt die Oberflaeche einen
  // Wert an die Rust-Kommandos, den deren `#[serde(untagged)]`-Enum nicht mehr kennt,
  // und JEDER local-stt-Aufruf scheitert an der Deserialisierung.
  test.each([
    ["am-parakeet-v2", { provider: "soniqo", model: "soniqo-parakeet-batch" }],
    ["am-parakeet-v3", { provider: "soniqo", model: "soniqo-parakeet-batch" }],
    [
      "am-whisper-large-v3",
      { provider: "whispercpp", model: "QuantizedLargeTurbo" },
    ],
  ])("rewrites the retired Argmax model %s", (stored, expected) => {
    expect(normalizeStoredSttSelection("anarlog", stored)).toEqual(expected);
    // Auch unter jedem anderen gespeicherten Anbieter, denn die Modellkennung ist
    // das, woran die Deserialisierung stirbt -- nicht der Anbietername.
    expect(normalizeStoredSttSelection("hyprnote", stored)).toEqual(expected);
  });

  // Der eigentliche Beweis: das Ziel der Umschreibung muss ein Modell sein, das die
  // App noch kennt. Eine Migration, die auf einen zweiten toten Wert zeigt, waere
  // eine Attrappe.
  test("every rewrite target is still a model the app accepts", () => {
    for (const stored of [
      "am-parakeet-v2",
      "am-parakeet-v3",
      "am-whisper-large-v3",
    ]) {
      const { model } = normalizeStoredSttSelection("anarlog", stored);
      expect(isSupportedLocalSttModel(model)).toBe(true);
    }
  });

  test("keeps the existing alias rewrites working", () => {
    expect(normalizeStoredSttModel("assemblyai", "universal")).toBe(
      "universal-3-5-pro",
    );
    expect(normalizeStoredSttModel("soniox", "stt-rt-v3")).toBe("stt-rt-v4");
  });
});

// Kimi/Grok-Review 02.09.2026 (F7). "anarlog" (und der aeltere Name
// "hyprnote") war die Kennung der Gegenstelle des Originals. Eine gespeicherte
// Wahl von deren Cloud-Modell kann in diesem Fork nie wieder verbinden --
// `isConfiguredSttModel` hielt sie trotzdem fuer eingerichtet, also gab es
// nicht einmal den Hinweis-Banner. Ziel ist derselbe lokale Standard, auf den
// die Batch-Transkription ohnehin zurueckfaellt (useRunBatch.ts).
describe("legacy hosted transcription selections", () => {
  test.each([
    ["anarlog", "cloud"],
    ["hyprnote", "cloud"],
    ["anarlog", undefined],
    ["anarlog", "something-unknown"],
  ])("moves %s / %s to the local default", (provider, model) => {
    expect(normalizeStoredSttSelection(provider, model)).toEqual({
      provider: "soniqo",
      model: "soniqo-parakeet-batch",
    });
  });

  // Grok 4 / Opus 4 (Review 02.09.2026): ein lokales Modell, das kein
  // Standard ist, bleibt bei der Umschreibung erhalten -- unter BEIDEN
  // Kennungen. Vorher griffen die Zweige fuer soniqo-* und apple-speech nur
  // fuer "anarlog"; "hyprnote" + "soniqo-parakeet-streaming" fiel auf den
  // Batch-Standard und verlor die Live-Wahl.
  test.each([
    ["anarlog", "soniqo-parakeet-streaming", "soniqo"],
    ["hyprnote", "soniqo-parakeet-streaming", "soniqo"],
    ["anarlog", "apple-speech", "apple_speech"],
    ["hyprnote", "apple-speech", "apple_speech"],
  ])(
    "keeps the local model %s / %s and moves it to %s",
    (provider, model, target) => {
      expect(normalizeStoredSttSelection(provider, model)).toEqual({
        provider: target,
        model,
      });
    },
  );

  test("moves a Whisper model stored under the hosted id to whispercpp", () => {
    expect(
      normalizeStoredSttSelection("anarlog", "QuantizedLargeTurbo"),
    ).toEqual({ provider: "whispercpp", model: "QuantizedLargeTurbo" });
    expect(
      normalizeStoredSttSelection("hyprnote", "QuantizedLargeTurbo"),
    ).toEqual({ provider: "whispercpp", model: "QuantizedLargeTurbo" });
  });

  test("leaves every other provider alone", () => {
    expect(normalizeStoredSttSelection("deepgram", "nova-3-general")).toEqual({
      provider: "deepgram",
      model: "nova-3-general",
    });
    expect(normalizeStoredSttSelection(undefined, undefined)).toEqual({
      provider: undefined,
      model: undefined,
    });
  });
});
