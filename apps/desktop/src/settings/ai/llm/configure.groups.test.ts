import { describe, expect, test } from "vitest";

import { LLM_LOCAL_PROVIDER_IDS, LLM_TOP_PROVIDER_IDS } from "./select";
import { PROVIDERS } from "./shared";
import { shouldShowInProviderList } from "./subscriptions";

import { filterProviders } from "~/settings/ai/shared/provider-search";
import { splitLocalTopMore } from "~/settings/ai/shared/provider-groups";

// Reproduziert, was configure.tsx ("Anbieter einrichten") tatsaechlich baut:
// erst der Sichtbarkeits-Filter (kein anarlog, gefaltete Abo-Zwillinge nur
// bei Suche) + die Suche, DANN dieselbe splitLocalTopMore-Quelle wie im
// Dropdown (select.tsx). Anders als bei STT stehen die vier lokalen Anbieter
// (LM Studio, Ollama, Unsloth, Apple Foundation) hier wirklich in der Liste,
// weil sie -- im Unterschied zu Soniqo/Apple Speech bei STT -- eine
// Einrichtungskarte mit Anleitung haben (28.09.2026).
function configuredCardProviders(search: string) {
  return filterProviders(
    PROVIDERS.filter((provider) => shouldShowInProviderList(provider.id, search)),
    search,
  );
}

describe("LLM configure list grouping (Anbieter einrichten)", () => {
  test("Local holds the four on-device providers, same order as the dropdown", () => {
    const { local } = splitLocalTopMore(
      configuredCardProviders(""),
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(local.map((provider) => provider.id)).toEqual([
      "apple_foundation",
      "lmstudio",
      "ollama",
      "unsloth",
    ]);
  });

  test("Cloud is the same fixed order as the dropdown", () => {
    const { top } = splitLocalTopMore(
      configuredCardProviders(""),
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(top.map((provider) => provider.id)).toEqual([
      "openrouter",
      "openai",
      "anthropic",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });

  test("a search that matches only a Local provider leaves Cloud and More empty", () => {
    const { local, top, more } = splitLocalTopMore(
      configuredCardProviders("ollama"),
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(local.map((provider) => provider.id)).toEqual(["ollama"]);
    expect(top).toEqual([]);
    expect(more).toEqual([]);
  });

  test("a search that matches only a More provider leaves Local and Cloud empty", () => {
    const { local, top, more } = splitLocalTopMore(
      configuredCardProviders("mistral"),
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(local).toEqual([]);
    expect(top).toEqual([]);
    expect(more.map((provider) => provider.id)).toEqual(["mistral"]);
  });

  test("folded subscription twins (chatgpt/claude/grok/kimi_code) only appear once searched", () => {
    expect(
      configuredCardProviders("").some((provider) => provider.id === "claude"),
    ).toBe(false);
    expect(
      configuredCardProviders("claude").some(
        (provider) => provider.id === "claude",
      ),
    ).toBe(true);
  });
});
