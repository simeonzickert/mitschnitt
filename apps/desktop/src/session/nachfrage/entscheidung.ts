/**
 * Wer war dabei, und wie heisst das Gespraech.
 *
 * Gemessen an 389 Sitzungen des produktiven anarlog: die 66 Sitzungen mit
 * Kalendertermin tragen in 66 von 66 Faellen den Termintitel wortgleich, sind
 * also von Hand geschrieben. Die 323 ohne Termin tragen einen, den ein
 * Sprachmodell aus dem Transkript gebaut hat -- daher kam "Serredi", ein Name,
 * den nie jemand gesagt hat.
 *
 * Daraus die Regel dieser Datei: gefragt wird nur, wo die Antwort nicht schon
 * dasteht. Eine Rueckfrage, die auch dann kommt, wenn Titel und Teilnehmer
 * bereits menschengeschrieben sind, wird nach dem dritten Mal weggeklickt und
 * ist damit wertlos.
 */

/** Der Ausschnitt der Sitzung, den die Entscheidung braucht. */
export type SitzungsStand = {
  /** `sessions.event_id` -- die Verknuepfung auf den Kalendertermin. */
  eventId: string | null;
  /** Das denormalisierte Event aus `sessions.event_json`. */
  event: { id?: string | null } | null;
};

/**
 * Ein Kalendertermin liegt auf zwei Wegen vor: als Fremdschluessel und als
 * mitgeschriebenes Event im JSON. Beide zaehlen -- wer nur einen prueft,
 * fragt in der Haelfte der Faelle ueberfluessig.
 */
export function hatKalendertermin(stand: SitzungsStand): boolean {
  if (stand.eventId !== null && stand.eventId.trim() !== "") {
    return true;
  }

  const eingebettet = stand.event?.id;
  return typeof eingebettet === "string" && eingebettet.trim() !== "";
}

/**
 * Die Frage kommt nur, wo sie etwas beitraegt. Hat das Gespraech einen
 * Kalendertermin, sind Titel und Teilnehmer schon menschengeschrieben.
 */
export function brauchtNachfrage(stand: SitzungsStand): boolean {
  return !hatKalendertermin(stand);
}

/**
 * Woher der Titelvorschlag kommt. Ein leeres Feld ist eine Aufgabe, ein
 * gefuellter Vorschlag ist eine Bestaetigung -- deshalb wird vorgeschlagen
 * und nicht abgefragt.
 *
 * Bewusst ohne Sprachmodell: zum Zeitpunkt der Frage ist der Titel-Lauf noch
 * gar nicht gelaufen, und genau dessen Erfindungen sind der Anlass.
 */
export type Vorschlag =
  | { art: "vorhanden"; titel: string }
  | { art: "mit-einem"; name: string }
  | { art: "mit-mehreren"; name: string; weitere: number }
  | { art: "nur-zeit" };

export type VorschlagsStand = {
  /** Der bereits gesetzte Sitzungstitel, oft leer bei spontanen Gespraechen. */
  titel: string;
  /** Namen der Teilnehmer, in der Reihenfolge der Oberflaeche. */
  teilnehmer: string[];
};

/**
 * Rangfolge aus dem, was vorliegt: ein vorhandener Titel gewinnt immer (er
 * ist entweder vom Menschen oder aus dem Kalender), danach die Teilnehmer,
 * zuletzt der Zeitpunkt.
 *
 * Gibt eine Beschreibung zurueck, keinen fertigen Satz -- der Text entsteht
 * in der Oberflaeche mit den Sprachkatalogen.
 */
export function titelVorschlag(stand: VorschlagsStand): Vorschlag {
  const titel = stand.titel.trim();
  if (titel !== "") {
    return { art: "vorhanden", titel };
  }

  const namen = stand.teilnehmer
    .map((name) => name.trim())
    .filter((name) => name !== "");

  if (namen.length === 1) {
    return { art: "mit-einem", name: namen[0]! };
  }

  if (namen.length > 1) {
    return { art: "mit-mehreren", name: namen[0]!, weitere: namen.length - 1 };
  }

  return { art: "nur-zeit" };
}
