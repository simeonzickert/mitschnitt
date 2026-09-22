/**
 * Ob der letzte abgeschlossene Schwung Woerterbuch-Schreibvorgaenge gehalten hat.
 *
 * Warum das ueberhaupt ein eigener Speicher ist und kein `useState` in der
 * Seite -- zwei Loecher, beide am 03.09.2026 gemessen:
 *
 * 1. DAS BANNER LOESCHTE SICH AM FALSCHEN ENDE. Es wurde beim BEGINN des
 *    naechsten Schreibvorgangs zurueckgesetzt. Zwei schnelle Klicks: A startet,
 *    B startet (und loescht dabei), A scheitert (und meldet). Zwei Fehlschlaege
 *    lasen sich als einer -- und schlimmer, in der umgekehrten Reihenfolge
 *    (A scheitert, dann startet B und raeumt das Banner weg) verschwand die
 *    Meldung ueber einen Verlust, den niemand mehr sah. Ein Zustand, der beim
 *    START einer Arbeit etwas ueber deren ERGEBNIS behauptet, luegt in beide
 *    Richtungen.
 * 2. EIN SEITENWECHSEL NAHM DAS BANNER MIT. Wer nach einem gescheiterten
 *    Schreibvorgang die Einstellungsseite verlaesst und zurueckkommt, sah eine
 *    heile Oberflaeche ueber einem verlorenen Eintrag.
 *
 * Die Regel hier ist deshalb: gerechnet wird ausschliesslich beim ABSCHLUSS,
 * und erst wenn NICHTS mehr laeuft. Solange noch ein Schreibvorgang unterwegs
 * ist, behaelt die Anzeige, was sie hat -- sie behauptet nie etwas ueber ein
 * Ergebnis, das es noch nicht gibt. Ist der Schwung durch, sagt sie genau eine
 * Sache: hat in diesem Schwung etwas nicht geklappt, ja oder nein.
 *
 * Ein erfolgreicher Schwung raeumt das Banner also weg. Das ist Absicht: der
 * Text sagt "bitte noch einmal versuchen", und der naechste geglueckte Versuch
 * ist die Antwort darauf.
 */
import { useSyncExternalStore } from "react";

let laufende = 0;
let fehlerImSchwung = false;
let sichtbar = false;
const zuhoerer = new Set<() => void>();

/**
 * Haengt einen Schreibvorgang an den laufenden Schwung.
 *
 * Reicht Erfolg wie Fehlschlag unveraendert weiter -- der Aufrufer MUSS den
 * Fehlerfall selbst behandeln. Genau daran haengt die Atomizitaet beim
 * Annehmen eines Vorschlags: der zweite Schreibvorgang darf nur laufen, wenn
 * der erste wirklich durchkam.
 */
export function beobachteSchreibvorgang<T>(vorgang: Promise<T>): Promise<T> {
  laufende += 1;
  return vorgang.then(
    (wert) => {
      schliesseAb(false);
      return wert;
    },
    (fehler: unknown) => {
      schliesseAb(true);
      throw fehler;
    },
  );
}

function schliesseAb(gescheitert: boolean): void {
  if (gescheitert) {
    fehlerImSchwung = true;
  }
  laufende -= 1;
  if (laufende > 0) {
    return;
  }
  const naechster = fehlerImSchwung;
  fehlerImSchwung = false;
  if (naechster === sichtbar) {
    return;
  }
  sichtbar = naechster;
  for (const zuhoerer_ of zuhoerer) {
    zuhoerer_();
  }
}

function abonniere(rufe: () => void): () => void {
  zuhoerer.add(rufe);
  return () => {
    zuhoerer.delete(rufe);
  };
}

const lies = () => sichtbar;

export function useSchreibfehler(): boolean {
  return useSyncExternalStore(abonniere, lies, lies);
}

/**
 * Nur fuer Tests: der Speicher lebt laenger als eine Ansicht, und genau das
 * ist sein Zweck -- ein Testlauf darf den vorigen trotzdem nicht erben.
 */
export function setzeSchreibstatusZurueck(): void {
  laufende = 0;
  fehlerImSchwung = false;
  sichtbar = false;
  for (const zuhoerer_ of zuhoerer) {
    zuhoerer_();
  }
}
