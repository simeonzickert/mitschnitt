/**
 * Wer als Teilnehmer zaehlt, wenn die Oberflaeche fragt "wer war dabei".
 *
 * Gemessen am Bestand am 04.09.2026, ueber die 9 Sitzungen ohne
 * Kalendertermin -- also genau die, in denen die Frage ueberhaupt kommt:
 *
 *   Name                       human_id == sessions.owner_user_id   Anzahl
 *   Mads Verlin                ja                                   9
 *   Nina Falkner               nein                                 2
 *   Nina Falkner | nordwerk    nein                                 1
 *   Jan Meinerz | nordwerk     nein                                 1
 *
 * Der Mensch am Mikrofon haengt also an JEDER dieser Sitzungen, und sein
 * Menschen-Datensatz traegt dieselbe Kennung wie der Besitzer der Sitzung
 * (dieselbe Gleichsetzung nutzt der Rest der App als `selfHumanId`, z. B.
 * `enhance-transform.ts`). Ohne diesen Abzug schlaegt der Dialog "Gespraech
 * mit Mads Verlin" als Titel vor und zaehlt ihn als jemanden, der dabei
 * war -- der Knopf "Nur ich, weiter" waere damit unerreichbar.
 *
 * Zweiter Abzug: ausdruecklich entfernte Teilnehmer. Dieselbe Regel wendet
 * der uebernommene Teilnehmer-Block aus dem Metadaten-Popover an; ohne sie
 * naehme der Titelvorschlag jemanden, den die Liste daneben gar nicht zeigt.
 */
export type TeilnehmerZeile = {
  humanId?: string | null;
  source?: string | null;
  name?: string | null;
};

export function fremdeTeilnehmerNamen(
  teilnehmer: readonly TeilnehmerZeile[],
  selbstHumanId: string | null,
): string[] {
  return teilnehmer
    .filter((eintrag) => eintrag.source !== "excluded")
    .filter(
      (eintrag) => !selbstHumanId || (eintrag.humanId ?? "") !== selbstHumanId,
    )
    .map((eintrag) => eintrag.name?.trim() ?? "")
    .filter((name) => name !== "");
}
