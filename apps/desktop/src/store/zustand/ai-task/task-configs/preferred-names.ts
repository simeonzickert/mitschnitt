import { dictionaryCanonicalTerms } from "~/stt/dictionary-entry";

/**
 * Die eingetragene Stichwortliste als Namensvorgabe an das Sprachmodell.
 *
 * Drei Dinge, die hier NICHT passieren duerfen -- und alle drei sind schon
 * passiert:
 *
 * 1. Die eingetragene Verhoerung darf das Modell nie zu sehen bekommen. Eine
 *    Zeile wie `Sedacz => Sarec` ging bis zum 03.09.2026 woertlich in den
 *    Prompt -- die Anweisung lautete dann, genau diese Zeichenkette exakt zu
 *    schreiben. Deshalb `dictionaryCanonicalTerms` und nicht die rohe Liste.
 * 2. Die Vorgabe "nimm diese Schreibweise, auch wenn das Transkript anders
 *    schreibt" ist eine Lizenz zum Ueberschreiben. Aus dem Verhoerer "Sarec"
 *    wurde im Sitzungstitel "Serredi" -- eine DRITTE Schreibweise, die weder
 *    im Transkript noch in der Liste stand.
 * 3. Und der teuerste Ausgang, gemessen am 03.09.2026 an der Sitzung
 *    `a1b2c3d4` (Entscheid 02.09.): das Modell tauscht einen Namen aus
 *    dem Material gegen einen ANDEREN Eintrag derselben Liste. Das Transkript
 *    sagte "Sarec", "Sedarac" und "aus dem Hause Sedolage"; auf der Liste
 *    standen sowohl "Sedacz" als auch "Phonowerk". Das Modell griff den
 *    klanglich naechsten Eintrag -- und schrieb eine vollstaendige
 *    Zusammenfassung ueber den falschen Kunden, inklusive Titel. Im Transkript
 *    selbst kommt "Phonowerk" kein einziges Mal vor.
 *
 * Deshalb ist die Ueberschreib-Lizenz seit dem 03.09.2026 raus: die Liste
 * nennt Schreibweisen, sie erlaubt keinen Namenstausch. Wer den Satz "even if
 * the transcript spells them differently" wieder einbaut, holt Fall 3 zurueck.
 */
export function formatPreferredNamesGuidance(terms: string[]): string {
  const normalized = dictionaryCanonicalTerms(terms);
  if (normalized.length === 0) {
    return "";
  }

  return `# Preferred Names

These names and terms are spelled as follows:
${normalized.map((term) => `- ${term}`).join("\n")}

Rules for this list:
- Use a spelling from the list only where the material clearly means that exact term. A similar sound alone is not enough.
- Never replace a name in the material with a different entry from this list.
- Never invent a spelling that appears neither in the material you were given nor in the list above.
- If you are unsure which name is meant, keep the spelling from the material or leave the name out.`;
}

export function appendPreferredNamesGuidance(
  prompt: string,
  terms: string[],
): string {
  const guidance = formatPreferredNamesGuidance(terms);
  return guidance ? `${prompt}\n\n${guidance}` : prompt;
}
