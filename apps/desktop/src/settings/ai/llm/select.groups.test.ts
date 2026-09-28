import { describe, expect, test } from "vitest";

import { splitLocalTopMore } from "~/settings/ai/shared/provider-groups";

import { LLM_LOCAL_PROVIDER_IDS, LLM_TOP_PROVIDER_IDS } from "./select";
import { PROVIDERS } from "./shared";

describe("splitLocalTopMore against the real LLM provider list", () => {
  test("local is exactly the four on-device providers, in order", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(result.local.map((provider) => provider.id)).toEqual([
      "apple_foundation",
      "lmstudio",
      "ollama",
      "unsloth",
    ]);
  });

  test("top is OpenRouter, OpenAI, Anthropic, Gemini, Groq, Custom, in exactly that order", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(result.top.map((provider) => provider.id)).toEqual([
      "openrouter",
      "openai",
      "anthropic",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });

  test("more contains none of the local/top ids and is sorted alphabetically by displayName", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );
    const placedIdSet = new Set<string>([
      ...LLM_LOCAL_PROVIDER_IDS,
      ...LLM_TOP_PROVIDER_IDS,
    ]);

    for (const provider of result.more) {
      expect(placedIdSet.has(provider.id)).toBe(false);
    }

    const displayNames = result.more.map((provider) => provider.displayName);
    expect(displayNames).toEqual(
      [...displayNames].sort((a, b) => a.localeCompare(b)),
    );
  });

  test("no provider is lost between local, top and more", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(result.local.length + result.top.length + result.more.length).toBe(
      PROVIDERS.length,
    );
  });

  test("a selected provider in more comes first, ahead of configured and the alphabetical rest", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
      { selectedId: "mistral", configuredIds: ["deepseek", "cohere"] },
    );
    const ids = result.more.map((provider) => provider.id);

    expect(ids[0]).toBe("mistral");
    expect(ids.slice(1, 3)).toEqual(["cohere", "deepseek"]);
  });
});
