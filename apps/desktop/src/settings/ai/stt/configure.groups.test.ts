import { describe, expect, test } from "vitest";

import { STT_LOCAL_PROVIDER_IDS, STT_TOP_PROVIDER_IDS } from "./select";
import { PROVIDERS } from "./shared";

import { filterProviders } from "~/settings/ai/shared/provider-search";
import { splitLocalTopMore } from "~/settings/ai/shared/provider-groups";

// Reproduziert, was configure.tsx ("Anbieter einrichten") tatsaechlich baut:
// erst der builtIn-Filter + die Suche, DANN dieselbe splitLocalTopMore-Quelle
// wie im Dropdown (select.tsx). Kein Komponenten-Render noetig -- die
// Gruppierung ist reine Logik (28.09.2026, Befund: die Karten waren "bunt
// gemischt").
function configuredCardProviders(search: string) {
  return filterProviders(
    PROVIDERS.filter((provider) => !("builtIn" in provider)),
    search,
  );
}

describe("STT configure list grouping (Anbieter einrichten)", () => {
  test("Local is always empty: builtIn/on-device providers need no setup card", () => {
    const { local } = splitLocalTopMore(
      configuredCardProviders(""),
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(local).toEqual([]);
  });

  test("Cloud is the same fixed order as the dropdown: OpenRouter, OpenAI, Gemini, Groq, Custom", () => {
    const { top } = splitLocalTopMore(
      configuredCardProviders(""),
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(top.map((provider) => provider.id)).toEqual([
      "openrouter",
      "openai",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });

  test("More holds every other cloud provider, alphabetical, none lost", () => {
    const providers = configuredCardProviders("");
    const { top, more } = splitLocalTopMore(
      providers,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(more.length).toBe(providers.length - top.length);
    const displayNames = more.map((provider) => provider.displayName);
    expect(displayNames).toEqual(
      [...displayNames].sort((a, b) => a.localeCompare(b)),
    );
  });

  test("a search that matches only a More provider leaves Cloud empty", () => {
    const { top, more } = splitLocalTopMore(
      configuredCardProviders("deepgram"),
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(top).toEqual([]);
    expect(more.map((provider) => provider.id)).toEqual(["deepgram"]);
  });

  test("a search that matches only a Cloud provider leaves More empty", () => {
    const { top, more } = splitLocalTopMore(
      configuredCardProviders("openai"),
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(top.map((provider) => provider.id)).toEqual(["openai"]);
    expect(more).toEqual([]);
  });
});
