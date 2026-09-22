export function normalizeKeywordList(words: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];

  for (const word of words) {
    const normalized = word.trim().replace(/\s+/g, " ");
    // Bewusst `toLowerCase`: derselbe Schluessel wie `dictionaryEntryKey` und
    // wie Rusts `to_lowercase`. Im tuerkischen Locale macht
    // `toLocaleLowerCase` aus "I" ein "\u0131" -- die Liste entdoppelte dann
    // nach einer anderen Regel als der Nachlauf nachschlaegt.
    const key = normalized.toLowerCase();
    if (normalized.length < 2 || seen.has(key)) {
      continue;
    }
    seen.add(key);
    result.push(normalized);
  }

  return result;
}

export function parseDictionaryTermsText(value: string): string[] {
  return normalizeKeywordList(
    value
      .split(/[\n,]/)
      .map((term) => term.trim())
      .filter(Boolean),
  );
}

export function formatDictionaryTerms(terms: string[]): string {
  return normalizeKeywordList(terms).join("\n");
}

export function parseDictionaryTermsJson(value: unknown): string[] {
  if (Array.isArray(value)) {
    return normalizeKeywordList(
      value.filter((term): term is string => typeof term === "string"),
    );
  }

  if (typeof value !== "string") {
    return [];
  }

  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed)
      ? normalizeKeywordList(
          parsed.filter((term): term is string => typeof term === "string"),
        )
      : [];
  } catch {
    return [];
  }
}
