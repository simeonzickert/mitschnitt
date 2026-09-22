import { describe, expect, test } from "vitest";

import { groupProviders } from "~/settings/ai/shared/provider-groups";
import { STT_PRIMARY_PROVIDER_IDS } from "~/settings/ai/stt/select";
import { PROVIDERS } from "~/settings/ai/stt/shared";

describe("groupProviders against the real STT provider list", () => {
  test("primary is exactly the nine main providers, in order", () => {
    const result = groupProviders(PROVIDERS, STT_PRIMARY_PROVIDER_IDS);

    expect(result.primary.map((provider) => provider.id)).toEqual([
      "soniqo",
      "apple_speech",
      "whispercpp",
      "local_file",
      "openai",
      "groq",
      "google_generative_ai",
      "elevenlabs",
      "custom",
    ]);
  });

  test("others contains none of the primary ids and is sorted alphabetically by displayName", () => {
    const result = groupProviders(PROVIDERS, STT_PRIMARY_PROVIDER_IDS);
    const primaryIdSet = new Set<string>(STT_PRIMARY_PROVIDER_IDS);

    for (const provider of result.others) {
      expect(primaryIdSet.has(provider.id)).toBe(false);
    }

    const displayNames = result.others.map((provider) => provider.displayName);
    const sortedDisplayNames = [...displayNames].sort((a, b) =>
      a.localeCompare(b),
    );
    expect(displayNames).toEqual(sortedDisplayNames);
  });

  test("a niche provider (gladia) sits in others, not primary, and gets pinned when selected", () => {
    const withoutSelection = groupProviders(PROVIDERS, STT_PRIMARY_PROVIDER_IDS);

    expect(
      withoutSelection.primary.some((provider) => provider.id === "gladia"),
    ).toBe(false);
    expect(
      withoutSelection.others.some((provider) => provider.id === "gladia"),
    ).toBe(true);
    expect(
      withoutSelection.pinned.some((provider) => provider.id === "gladia"),
    ).toBe(false);

    const withSelection = groupProviders(PROVIDERS, STT_PRIMARY_PROVIDER_IDS, {
      selectedId: "gladia",
    });

    expect(withSelection.pinned.map((provider) => provider.id)).toEqual([
      "gladia",
    ]);
  });

  test("no provider is lost between primary and others", () => {
    const result = groupProviders(PROVIDERS, STT_PRIMARY_PROVIDER_IDS);

    expect(result.primary.length + result.others.length).toBe(
      PROVIDERS.length,
    );

    const groupedIds = new Set([
      ...result.primary.map((provider) => provider.id),
      ...result.others.map((provider) => provider.id),
    ]);
    const allIds = new Set(PROVIDERS.map((provider) => provider.id));
    expect(groupedIds).toEqual(allIds);
  });
});
