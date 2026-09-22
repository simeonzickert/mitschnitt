import { describe, expect, it } from "vitest";

import { normalizeStoredLlmSelection } from "./llm-selection";

// F7 (Kimi/Grok-Review 02.09.2026). Der Verhaltensbeweis "gespeicherte
// Kennung anarlog -> vorher Fehler beim ersten Aufruf, nachher leer + Banner"
// liegt in settings/queries.test.tsx (dort war er rot); dieser Test haelt
// nur den Vertrag des Helfers fest.
describe("normalizeStoredLlmSelection", () => {
  it.each(["anarlog", "hyprnote"])(
    "clears the hosted provider id %s together with its model",
    (provider) => {
      expect(normalizeStoredLlmSelection(provider, "Auto")).toEqual({
        provider: "",
        model: "",
      });
    },
  );

  it("leaves a real provider and its model alone", () => {
    expect(
      normalizeStoredLlmSelection("openrouter", "openai/gpt-5.6-terra"),
    ).toEqual({ provider: "openrouter", model: "openai/gpt-5.6-terra" });
  });

  it("does not invent a value where none was stored", () => {
    expect(normalizeStoredLlmSelection(undefined, undefined)).toEqual({
      provider: undefined,
      model: undefined,
    });
    expect(normalizeStoredLlmSelection("", "")).toEqual({
      provider: "",
      model: "",
    });
  });
});
