import { create } from "zustand";

import { brauchtNachfrage } from "./entscheidung";

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

type Abhaengigkeiten = {
  standLaden: (sessionId: string) => Promise<{
    eventId: string | null;
    event: { id?: string | null } | null;
  } | null>;
};

const echt: Abhaengigkeiten = {
  standLaden: async (sessionId) => {
    const schnappschuss = await loadSessionContentSnapshot(sessionId);
    if (!schnappschuss) {
      return null;
    }
    return {
      eventId: schnappschuss.eventId,
      event: (schnappschuss.event ?? null) as { id?: string | null } | null,
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
