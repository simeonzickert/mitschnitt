/**
 * Was die Antwort auf die Nachfrage in der Datenbank bewirkt.
 *
 * Bewusst reine Orchestrierung mit eingereichten Abhaengigkeiten: die Regeln
 * darin (was ist eine Aenderung, was ist ein Name, was passiert ohne Besitzer)
 * sind die eigentliche Arbeit und muessen ohne Datenbank pruefbar sein.
 *
 * Warum der Titel zaehlt: der Titel-Lauf erfindet nur, wo das Feld leer ist
 * (`ai-task/task-configs/enhance-success.ts`, `if (!trimmedTitle ...)`). Ein
 * hier eingetragener Titel schuetzt sich damit selbst -- es braucht keinen
 * Eingriff in die Titelkette.
 *
 * Warum die Namen zaehlen: Teilnehmer speisen die Stichwortliste des Erkenners
 * und die Anker-Menge der Woerterbuch-Vorschlaege. Ein hier eingetragener Name
 * ist kuenftig ein Beleg, keine Verhoerung.
 */

export type SpeicherEingabe = {
  sessionId: string;
  /** `sessions.user_id` -- ohne den kann kein Mensch angelegt werden. */
  ownerUserId: string | null;
  /** Der Titel aus dem Feld, so wie er dasteht. */
  titel: string;
  /** Der Titel, der vor dem Dialog in der Sitzung stand. */
  titelVorher: string;
  /** Frisch eingetippte Namen, in der Reihenfolge der Eingabe. */
  neueNamen: string[];
  /** Namen, die bereits an der Sitzung haengen. */
  vorhandeneNamen: string[];
};

export type SpeicherAbhaengigkeiten = {
  titelSetzen: (sessionId: string, titel: string) => Promise<void>;
  menschAnlegen: (input: {
    ownerUserId: string;
    name: string;
  }) => Promise<string>;
  teilnehmerAnhaengen: (sessionId: string, humanId: string) => Promise<void>;
};

export type SpeicherErgebnis = {
  titelGeschrieben: boolean;
  angelegteNamen: string[];
};

const normal = (wert: string) => wert.trim().toLocaleLowerCase();

/**
 * Schreibt genau das, was sich geaendert hat. Ein Fehler an einem Namen laesst
 * die uebrigen Namen und den Titel stehen -- die Antwort war richtig, nur ein
 * Schreibvorgang ging schief, und ein Teilerfolg ist besser als ein Rueckbau.
 */
export async function nachfrageSpeichern(
  eingabe: SpeicherEingabe,
  abhaengigkeiten: SpeicherAbhaengigkeiten,
): Promise<SpeicherErgebnis> {
  const ergebnis: SpeicherErgebnis = {
    titelGeschrieben: false,
    angelegteNamen: [],
  };

  const titel = eingabe.titel.trim();
  // Ein leeres Feld ist keine Antwort: es wuerde einen vorhandenen Titel
  // loeschen und dem Modell die Erfindung erst wieder erlauben.
  if (titel !== "" && titel !== eingabe.titelVorher.trim()) {
    try {
      await abhaengigkeiten.titelSetzen(eingabe.sessionId, titel);
      ergebnis.titelGeschrieben = true;
    } catch (fehler) {
      console.error("[nachfrage] Titel nicht gespeichert", fehler);
    }
  }

  const ownerUserId = eingabe.ownerUserId;
  if (!ownerUserId) {
    return ergebnis;
  }

  const gesehen = new Set(eingabe.vorhandeneNamen.map(normal));
  for (const roh of eingabe.neueNamen) {
    const name = roh.trim();
    if (name === "" || gesehen.has(normal(name))) {
      continue;
    }
    gesehen.add(normal(name));

    try {
      const humanId = await abhaengigkeiten.menschAnlegen({
        ownerUserId,
        name,
      });
      await abhaengigkeiten.teilnehmerAnhaengen(eingabe.sessionId, humanId);
      ergebnis.angelegteNamen.push(name);
    } catch (fehler) {
      console.error("[nachfrage] Teilnehmer nicht gespeichert", name, fehler);
    }
  }

  return ergebnis;
}
