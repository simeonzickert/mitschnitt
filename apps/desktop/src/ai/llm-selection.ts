/**
 * Die Kennungen, unter denen das Original seine eigene Sprachmodell-
 * Gegenstelle fuehrte ("anarlog", davor "hyprnote"). Der Fork spricht mit
 * keinem solchen Server; die Anbieterliste filtert den Eintrag (llm/shared.tsx),
 * aber eine Bestandsinstallation kann die Kennung noch gespeichert haben, und
 * ein spaeterer Import alter Daten bringt sie wieder mit.
 *
 * Anders als bei der Transkription gibt es keinen lokalen Standard, auf den
 * sich das Modell stillschweigend umbiegen liesse (Apple Foundation ist an
 * Plattform und macOS-Version gebunden). Die Wahl wird deshalb geleert: der
 * Hinweis-Banner (contexts/notifications.tsx) sagt dann, dass ein Anbieter zu
 * waehlen ist, statt dass die erste Zusammenfassung in einem Fehler endet
 * (Kimi/Grok-Review 02.09.2026, F7).
 */
const LEGACY_HOSTED_LLM_PROVIDER_IDS: ReadonlySet<string> = new Set([
  "anarlog",
  "hyprnote",
]);

export function normalizeStoredLlmSelection(
  provider: string | undefined,
  model: string | undefined,
): { provider: string | undefined; model: string | undefined } {
  if (provider && LEGACY_HOSTED_LLM_PROVIDER_IDS.has(provider)) {
    return { provider: "", model: "" };
  }

  return { provider, model };
}
