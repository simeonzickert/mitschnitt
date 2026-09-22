import { describe, expect, test } from "vitest";

import { processGenericModels } from "./list-openai";

describe("processGenericModels", () => {
  test("keeps Cohere model versions while still filtering non-chat models", () => {
    const result = processGenericModels(
      [{ id: "command-a-plus-05-2026" }, { id: "embed-v4.0" }],
      { filterDateSnapshots: false },
    );

    expect(result.models).toEqual(["command-a-plus-05-2026"]);
    expect(result.ignored).toEqual([
      { id: "embed-v4.0", reasons: ["common_keyword"] },
    ]);
  });

  test("filters date snapshots by default for generic providers", () => {
    const result = processGenericModels([{ id: "model-05-2026" }]);

    expect(result.models).toEqual([]);
    expect(result.ignored).toEqual([
      { id: "model-05-2026", reasons: ["date_snapshot"] },
    ]);
  });

  test("filters dotted Bedrock model IDs by their model name", () => {
    const result = processGenericModels([
      { id: "anthropic.claude-opus-4.7" },
      { id: "google.gemma-3-27b-it" },
      { id: "anthropic.claude-opus-5" },
    ]);

    expect(result.models).toEqual([
      "anthropic.claude-opus-5",
      "google.gemma-3-27b-it",
    ]);
    expect(result.ignored).toEqual([
      { id: "anthropic.claude-opus-4.7", reasons: ["old_model"] },
    ]);
  });
});
