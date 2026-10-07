import { describe, expect, it } from "vitest";

import { getSummaryMaxOutputTokens } from "./enhance-workflow";

describe("getSummaryMaxOutputTokens", () => {
  it("allows 64000 tokens for direct Anthropic providers", () => {
    expect(getSummaryMaxOutputTokens("anthropic.messages")).toBe(64_000);
  });

  it("sets no explicit limit for the Apple Foundation model", () => {
    expect(getSummaryMaxOutputTokens("apple_foundation")).toBeUndefined();
  });

  it.each([
    "openrouter.chat",
    "openai.responses",
    "google.generative-ai",
    "lmstudio",
    "ollama",
    "",
  ])("caps %s at 32000 tokens", (provider) => {
    expect(getSummaryMaxOutputTokens(provider)).toBe(32_000);
  });
});
