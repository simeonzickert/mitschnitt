import { describe, expect, test } from "vitest";

import { LLM_PRIMARY_PROVIDER_IDS } from "~/settings/ai/llm/select";
import { PROVIDERS } from "~/settings/ai/llm/shared";

import { groupProviders } from "./provider-groups";

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

describe("groupProviders against the real LLM provider list", () => {
  test("primary is exactly the six main providers, in order", () => {
    const result = groupProviders(PROVIDERS, LLM_PRIMARY_PROVIDER_IDS);

    expect(result.primary.map((provider) => provider.id)).toEqual([
      "anthropic",
      "openai",
      "openrouter",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });
});
