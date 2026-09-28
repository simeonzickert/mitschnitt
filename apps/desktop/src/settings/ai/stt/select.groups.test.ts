import { describe, expect, test } from "vitest";

import { splitLocalTopMore } from "~/settings/ai/shared/provider-groups";
import {
  getSelectableSttProviders,
  STT_LOCAL_PROVIDER_IDS,
  STT_TOP_PROVIDER_IDS,
} from "~/settings/ai/stt/select";
import { PROVIDERS } from "~/settings/ai/stt/shared";

describe("splitLocalTopMore against the real STT provider list", () => {
  test("Windows offers cloud providers without the unsupported local models", () => {
    const windowsProviders = getSelectableSttProviders("windows");

    expect(
      windowsProviders.some(
        (provider) => "builtIn" in provider && provider.builtIn,
      ),
    ).toBe(false);
    expect(
      windowsProviders.some((provider) => provider.id === "deepgram"),
    ).toBe(true);
    expect(getSelectableSttProviders("macos")).toEqual(PROVIDERS);
    expect(getSelectableSttProviders("linux")).toEqual(PROVIDERS);
  });

  test("local is exactly the four on-device providers, Apple Speech last", () => {
    // Apple Speech steht bewusst an letzter Stelle (28.09.2026): es
    // funktioniert, ist aber schwaecher als Parakeet/Soniqo -- siehe die
    // Warnung an seinem Eintrag (LowQualityBadge in select.tsx).
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(result.local.map((provider) => provider.id)).toEqual([
      "soniqo",
      "whispercpp",
      "local_file",
      "apple_speech",
    ]);
  });

  test("top is OpenRouter, OpenAI, Gemini, Groq, Custom, in exactly that order", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(result.top.map((provider) => provider.id)).toEqual([
      "openrouter",
      "openai",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });

  test("Windows has no local group, but keeps the same top order", () => {
    const windowsProviders = getSelectableSttProviders("windows");
    const result = splitLocalTopMore(
      windowsProviders,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(result.local).toEqual([]);
    expect(result.top.map((provider) => provider.id)).toEqual([
      "openrouter",
      "openai",
      "google_generative_ai",
      "groq",
      "custom",
    ]);
  });

  test("more contains none of the local/top ids and is sorted alphabetically by displayName", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );
    const placedIdSet = new Set<string>([
      ...STT_LOCAL_PROVIDER_IDS,
      ...STT_TOP_PROVIDER_IDS,
    ]);

    for (const provider of result.more) {
      expect(placedIdSet.has(provider.id)).toBe(false);
    }

    const displayNames = result.more.map((provider) => provider.displayName);
    const sortedDisplayNames = [...displayNames].sort((a, b) =>
      a.localeCompare(b),
    );
    expect(displayNames).toEqual(sortedDisplayNames);
  });

  test("a niche provider (gladia) sits in more, not local or top", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(result.local.some((provider) => provider.id === "gladia")).toBe(
      false,
    );
    expect(result.top.some((provider) => provider.id === "gladia")).toBe(
      false,
    );
    expect(result.more.some((provider) => provider.id === "gladia")).toBe(
      true,
    );
  });

  test("elevenlabs moved out of the fixed top order, into more", () => {
    // Vor dem Umbau (28.09.2026) stand elevenlabs in der festen Reihe;
    // die neue Vorgabe nennt nur OpenRouter/OpenAI/Gemini/Groq/Custom.
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    expect(result.top.some((provider) => provider.id === "elevenlabs")).toBe(
      false,
    );
    expect(
      result.more.some((provider) => provider.id === "elevenlabs"),
    ).toBe(true);
  });

  test("a selected provider in more comes first, ahead of configured and the alphabetical rest", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
      { selectedId: "gladia", configuredIds: ["deepgram", "cartesia"] },
    );
    const ids = result.more.map((provider) => provider.id);

    expect(ids[0]).toBe("gladia");
    expect(ids.slice(1, 3)).toEqual(["cartesia", "deepgram"]);
    expect(ids.slice(3)).not.toContain("gladia");
    expect(ids.slice(3)).not.toContain("cartesia");
    expect(ids.slice(3)).not.toContain("deepgram");
  });

  test("without options, more stays fully alphabetical (backward compatible)", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
    );

    const displayNames = result.more.map((provider) => provider.displayName);
    const sortedDisplayNames = [...displayNames].sort((a, b) =>
      a.localeCompare(b),
    );
    expect(displayNames).toEqual(sortedDisplayNames);
  });

  test("no provider is lost between local, top and more", () => {
    const result = splitLocalTopMore(
      PROVIDERS,
      STT_LOCAL_PROVIDER_IDS,
      STT_TOP_PROVIDER_IDS,
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
