import { create } from "zustand";

import { brauchtNachfrage } from "./entscheidung";

import { executeTransaction, liveQueryClient } from "~/db";
import { enqueueDatabaseWrite } from "~/db/write-queue";
import { loadSessionContentSnapshot } from "~/session/content-queries";

/**
 * Die Schleuse zwischen dem Ende der Aufnahme und dem Start der
 * Zusammenfassung.
 *
 * Zwei Zusagen, die den Bau bestimmen:
 *
 * 1. Aufnahme und Transkript werden nie blockiert. Der Aufruf sitzt im
 *    Lebenszyklus NACH der Transkript-Persistenz und nach dem Merker, der die
 *    ausstehende Zusammenfassung wiederherstellbar macht.
 *
 * 2. Es wird nur gewartet, wenn wirklich jemand fragt. Ein Zeitfenster waere
 *    geraten -- die Anmeldung des Dialogs ist gemessen. Meldet sich der Dialog
 *    ab (Fenster zu, Sitzung verlassen), werden offene Fragen aufgeloest,
 *    damit die Zusammenfassung nicht haengt.
 *
 * Bekannte Grenze, bewusst so: der Zustand lebt pro Fenster. Laeuft der
 * Lebenszyklus in einem Fenster ohne montierten Dialog, kommt keine Frage und
 * alles laeuft wie vorher. Das ist die sichere Fehlerrichtung.
 */

type Offen = {
  sessionId: string;
  aufloesen: () => void;
};

type NachfrageZustand = {
  /** Anzahl montierter Dialoge. Ohne einen wird nicht gewartet. */
  frager: number;
  /** Die eine offene Frage, oder keine. Nie zwei gleichzeitig. */
  offen: Offen | null;
};

export const useNachfrageStore = create<NachfrageZustand>(() => ({
  frager: 0,
  offen: null,
}));

/** Der Dialog meldet sich an, solange er montiert ist. */
export function frageranmelden(): () => void {
  useNachfrageStore.setState((zustand) => ({ frager: zustand.frager + 1 }));

  let abgemeldet = false;
  return () => {
    if (abgemeldet) {
      return;
    }
    abgemeldet = true;

    useNachfrageStore.setState((zustand) => {
      const frager = Math.max(0, zustand.frager - 1);
      if (frager > 0 || !zustand.offen) {
        return { frager };
      }
      // Der letzte Frager geht: eine offene Frage wuerde nie beantwortet.
      zustand.offen.aufloesen();
      return { frager, offen: null };
    });
  };
}

/** Beantwortet oder weggeklickt -- beides laesst die Zusammenfassung laufen. */
export function nachfrageErledigt(sessionId: string): void {
  useNachfrageStore.setState((zustand) => {
    if (!zustand.offen || zustand.offen.sessionId !== sessionId) {
      return {};
    }
    zustand.offen.aufloesen();
    return { offen: null };
  });
}

/**
 * Setzt den Merker "einmal beantwortet, fuer immer" fuer GENAU diese
 * Sitzung -- siehe die Doktrin in `entscheidung.ts`. Wird von `host.tsx` bei
 * jedem Ausgang aus dem Dialog aufgerufen: Bestaetigen genauso wie
 * Ueberspringen (Skip-Knopf, Schliessen-X, Escape, Klick daneben fuehren
 * alle auf denselben `onVerwerfen`-Pfad, siehe `dialog.tsx`).
 *
 * Braucht keine Migration: `sessions.metadata_json` existiert bereits
 * (`crates/db-app/migrations/20260710223922_canonical_data_model.sql`), der
 * Merker ist ein einzelnes Feld darin.
 *
 * Schluckt eigene Fehler, wie `nachfrageSpeichern` in `speichern.ts` es
 * schon fuer Titel und Teilnehmer tut: ein Schreibfehler hier darf die
 * Zusammenfassung nicht aufhalten. Im schlimmsten Fall fragt ein spaeterer
 * Stopp derselben Sitzung einmal zu viel -- ein ertraeglicher Ausgang,
 * keine haengende Zusammenfassung.
 */
export async function markiereNachfrageBeantwortet(
  sessionId: string,
): Promise<void> {
  try {
    const jetzt = new Date().toISOString();
    await enqueueDatabaseWrite(`session:${sessionId}`, () =>
      executeTransaction([
        {
          sql: `
            UPDATE sessions
            SET
              metadata_json = json_set(
                CASE WHEN json_valid(metadata_json) THEN metadata_json ELSE '{}' END,
                '$.nachfrageBeantwortetAm',
                ?
              ),
              updated_at = ?
            WHERE id = ? AND deleted_at IS NULL
          `,
          params: [jetzt, jetzt, sessionId],
        },
      ]),
    );
  } catch (error) {
    console.error("[nachfrage] Merker nicht gespeichert", error);
  }
}

type Abhaengigkeiten = {
  standLaden: (sessionId: string) => Promise<{
    eventId: string | null;
    event: { id?: string | null } | null;
    bereitsBeantwortet: boolean;
  } | null>;
};

const echt: Abhaengigkeiten = {
  standLaden: async (sessionId) => {
    const [schnappschuss, metadatenZeilen] = await Promise.all([
      loadSessionContentSnapshot(sessionId),
      liveQueryClient.execute<{ beantwortet_am: string | null }>(
        `
          SELECT
            NULLIF(json_extract(metadata_json, '$.nachfrageBeantwortetAm'), '') AS beantwortet_am
          FROM sessions
          WHERE id = ? AND deleted_at IS NULL
          LIMIT 1
        `,
        [sessionId],
      ),
    ]);
    if (!schnappschuss) {
      return null;
    }
    return {
      eventId: schnappschuss.eventId,
      event: (schnappschuss.event ?? null) as { id?: string | null } | null,
      bereitsBeantwortet: Boolean(metadatenZeilen[0]?.beantwortet_am),
    };
  },
};

/**
 * Wird vom Aufnahme-Lebenszyklus abgewartet, bevor die Zusammenfassung
 * startet. Kehrt in jedem Zweifelsfall sofort zurueck: kein Dialog montiert,
 * schon eine Frage offen, Kalendertermin vorhanden, oder ein Fehler beim
 * Laden. Eine nicht gestellte Frage ist ein ertraeglicher Ausgang, eine nie
 * startende Zusammenfassung nicht.
 */
export async function frageVorZusammenfassung(
  sessionId: string,
  abhaengigkeiten: Abhaengigkeiten = echt,
): Promise<void> {
  const zustand = useNachfrageStore.getState();
  if (zustand.frager === 0 || zustand.offen) {
    return;
  }

  let stand: Awaited<ReturnType<Abhaengigkeiten["standLaden"]>>;
  try {
    stand = await abhaengigkeiten.standLaden(sessionId);
  } catch (error) {
    console.error("[nachfrage] Sitzungsstand nicht lesbar", error);
    return;
  }

  if (!stand || !brauchtNachfrage(stand)) {
    return;
  }

  // Zwischen Laden und Setzen kann ein zweiter Lauf zugeschlagen haben.
  if (useNachfrageStore.getState().offen) {
    return;
  }

  await new Promise<void>((aufloesen) => {
    useNachfrageStore.setState({ offen: { sessionId, aufloesen } });
  });
}
