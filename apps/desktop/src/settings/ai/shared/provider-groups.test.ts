import { describe, expect, test } from "vitest";

import {
  LLM_LOCAL_PROVIDER_IDS,
  LLM_TOP_PROVIDER_IDS,
} from "~/settings/ai/llm/select";
import { PROVIDERS } from "~/settings/ai/llm/shared";

import { groupProviders, splitLocalTopMore } from "./provider-groups";

describe("groupProviders", () => {
  test("orders primary by primaryIds, not the input order", () => {
    const result = groupProviders(
      [
        { id: "openai", displayName: "OpenAI" },
        { id: "anthropic", displayName: "Anthropic" },
        { id: "groq", displayName: "Groq" },
      ],
      ["anthropic", "openai", "groq"],
    );

    expect(result.primary.map((provider) => provider.id)).toEqual([
      "anthropic",
      "openai",
      "groq",
    ]);
  });

  test("skips a primary id that has no matching provider", () => {
    const result = groupProviders(
      [{ id: "openai", displayName: "OpenAI" }],
      ["anthropic", "openai", "does-not-exist"],
    );

    expect(result.primary.map((provider) => provider.id)).toEqual(["openai"]);
  });

  test("sorts others alphabetically by displayName", () => {
    const result = groupProviders(
      [
        { id: "zai", displayName: "Zai" },
        { id: "alibaba_cloud", displayName: "Alibaba Cloud" },
        { id: "mistral", displayName: "Mistral" },
        { id: "anthropic", displayName: "Anthropic" },
      ],
      ["anthropic"],
    );

    expect(result.others.map((provider) => provider.id)).toEqual([
      "alibaba_cloud",
      "mistral",
      "zai",
    ]);
  });

  test("pins the selected provider while keeping it in others", () => {
    const result = groupProviders(
      [
        { id: "anthropic", displayName: "Anthropic" },
        { id: "mistral", displayName: "Mistral" },
        { id: "zai", displayName: "Zai" },
      ],
      ["anthropic"],
      { selectedId: "mistral" },
    );

    expect(result.pinned.map((provider) => provider.id)).toEqual(["mistral"]);
    expect(result.others.map((provider) => provider.id)).toEqual([
      "mistral",
      "zai",
    ]);
  });

  test("pins a configured provider", () => {
    const result = groupProviders(
      [
        { id: "anthropic", displayName: "Anthropic" },
        { id: "mistral", displayName: "Mistral" },
        { id: "zai", displayName: "Zai" },
      ],
      ["anthropic"],
      { configuredIds: ["zai"] },
    );

    expect(result.pinned.map((provider) => provider.id)).toEqual(["zai"]);
  });

  test("pinned is empty without options", () => {
    const result = groupProviders(
      [
        { id: "anthropic", displayName: "Anthropic" },
        { id: "mistral", displayName: "Mistral" },
      ],
      ["anthropic"],
    );

    expect(result.pinned).toEqual([]);
  });
});

describe("splitLocalTopMore", () => {
  test("splits into local (by localIds order), top (by topIds order) and more (alphabetical rest)", () => {
    const result = splitLocalTopMore(
      [
        { id: "ollama", displayName: "Ollama" },
        { id: "openai", displayName: "OpenAI" },
        { id: "zai", displayName: "Zai" },
        { id: "lmstudio", displayName: "LM Studio" },
        { id: "anthropic", displayName: "Anthropic" },
      ],
      ["lmstudio", "ollama"],
      ["openai", "anthropic"],
    );

    expect(result.local.map((provider) => provider.id)).toEqual([
      "lmstudio",
      "ollama",
    ]);
    expect(result.top.map((provider) => provider.id)).toEqual([
      "openai",
      "anthropic",
    ]);
    expect(result.more.map((provider) => provider.id)).toEqual(["zai"]);
  });

  test("an id in both localIds and topIds only ever lands in local", () => {
    // localIds wird zuerst aus der Liste gezogen; topIds sieht danach nur
    // noch den Rest -- ein Anbieter kann nicht in zwei Gruppen zugleich sein.
    const result = splitLocalTopMore(
      [{ id: "ollama", displayName: "Ollama" }],
      ["ollama"],
      ["ollama"],
    );

    expect(result.local.map((provider) => provider.id)).toEqual(["ollama"]);
    expect(result.top).toEqual([]);
  });

  test("missing local providers leave an empty local group without error", () => {
    const result = splitLocalTopMore(
      [{ id: "openai", displayName: "OpenAI" }],
      ["ollama", "lmstudio"],
      ["openai"],
    );

    expect(result.local).toEqual([]);
    expect(result.top.map((provider) => provider.id)).toEqual(["openai"]);
  });

  test("without options, more stays alphabetical (unchanged)", () => {
    const result = splitLocalTopMore(
      [
        { id: "zai", displayName: "Zai" },
        { id: "mistral", displayName: "Mistral" },
        { id: "cohere", displayName: "Cohere" },
      ],
      [],
      [],
    );

    expect(result.more.map((provider) => provider.id)).toEqual([
      "cohere",
      "mistral",
      "zai",
    ]);
  });

  test("with options, more puts the selected provider first, then configured providers, then the rest -- each bucket staying alphabetical", () => {
    const result = splitLocalTopMore(
      [
        { id: "zai", displayName: "Zai" },
        { id: "mistral", displayName: "Mistral" },
        { id: "cohere", displayName: "Cohere" },
        { id: "gladia", displayName: "Gladia" },
      ],
      [],
      [],
      { selectedId: "zai", configuredIds: ["mistral", "cohere"] },
    );

    // Alphabetische Ausgangsreihenfolge waere cohere, gladia, mistral, zai --
    // ausgewaehlt (zai) zuerst, dann eingerichtet (cohere vor mistral, weil
    // das die alphabetische Reihenfolge INNERHALB des Eimers ist), dann Rest.
    expect(result.more.map((provider) => provider.id)).toEqual([
      "zai",
      "cohere",
      "mistral",
      "gladia",
    ]);
  });

  test("a selected provider that is also in configuredIds appears only once, at the front", () => {
    const result = splitLocalTopMore(
      [
        { id: "zai", displayName: "Zai" },
        { id: "mistral", displayName: "Mistral" },
      ],
      [],
      [],
      { selectedId: "zai", configuredIds: ["zai", "mistral"] },
    );

    expect(result.more.map((provider) => provider.id)).toEqual([
      "zai",
      "mistral",
    ]);
  });
});

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

  test("anthropic sits in the fixed top order again, directly behind OpenAI", () => {
    // Erst raus (28.09.2026, "nur OpenRouter/OpenAI/Gemini/Groq/Custom"),
    // dann zurueck ("anthropic auch mit hoch", selber Tag) -- diesmal direkt
    // hinter OpenAI, nicht mehr an erster Stelle.
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );
    const topIds = result.top.map((provider) => provider.id);

    expect(topIds.indexOf("anthropic")).toBe(topIds.indexOf("openai") + 1);
    expect(
      result.more.some((provider) => provider.id === "anthropic"),
    ).toBe(false);
  });

  test("a selected provider in more comes first, ahead of configured and the alphabetical rest", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
      { selectedId: "zai", configuredIds: ["mistral", "cohere"] },
    );
    const ids = result.more.map((provider) => provider.id);

    expect(ids[0]).toBe("zai");
    expect(ids.slice(1, 3)).toEqual(["cohere", "mistral"]);
    expect(ids.slice(3)).not.toContain("zai");
    expect(ids.slice(3)).not.toContain("cohere");
    expect(ids.slice(3)).not.toContain("mistral");
  });

  test("no provider is lost between local, top and more", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      LLM_LOCAL_PROVIDER_IDS,
      LLM_TOP_PROVIDER_IDS,
    );

    expect(
      result.local.length + result.top.length + result.more.length,
    ).toBe(PROVIDERS.length);

    const groupedIds = new Set([
      ...result.local.map((provider) => provider.id),
      ...result.top.map((provider) => provider.id),
      ...result.more.map((provider) => provider.id),
    ]);
    const allIds = new Set(PROVIDERS.map((provider) => provider.id));
    expect(groupedIds).toEqual(allIds);
  });
});
