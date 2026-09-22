const dottedModelProviders = new Set([
  "ai21",
  "amazon",
  "anthropic",
  "cohere",
  "deepseek",
  "google",
  "meta",
  "minimax",
  "mistral",
  "moonshot",
  "nvidia",
  "openai",
  "qwen",
  "stability",
  "twelvelabs",
  "writer",
  "xai",
  "zai",
]);

export const modelName = (id: string): string => {
  const name = id.trim().replace(/^~/, "").toLowerCase().split("/").pop()!;
  const segments = name.split(".");
  const providerIndex = segments.findIndex((segment) =>
    dottedModelProviders.has(segment),
  );

  return providerIndex >= 0 && providerIndex < segments.length - 1
    ? segments.slice(providerIndex + 1).join(".")
    : name;
};
