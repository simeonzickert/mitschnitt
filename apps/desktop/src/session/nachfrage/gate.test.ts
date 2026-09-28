import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Fuer den zweiten Block unten ("echte Verdrahtung"): eine winzige,
 * In-Memory-Nachbildung von `sessions.metadata_json`, keyed nach Sitzung.
 * `vi.mock`-Fabriken werden nach oben gehoben, deshalb muss die Map selbst
 * per `vi.hoisted` hochgehoben werden, um in ihnen sichtbar zu sein --
 * dasselbe Muster wie die `mocks`-Objekte in `dialog.test.tsx`.
 */
const metadatenSpeicher = vi.hoisted(() => new Map<string, string>());

// `gate.ts`s `echt`-Pfad (der DEFAULT, wenn kein `abhaengigkeiten`-Argument
// mitgegeben wird) ruft genau diese drei Module auf. Der erste Testblock
// unten benutzt sie nie -- er reicht immer sein eigenes `abhaengigkeiten`
// mit -- deshalb stoeren diese Mocks ihn nicht.
vi.mock("~/session/content-queries", () => ({
  loadSessionContentSnapshot: vi.fn(async (sessionId: string) =>
    sessionId === "fehlt" ? null : { eventId: null, event: null },
  ),
}));

vi.mock("~/db", () => ({
  liveQueryClient: {
    execute: vi.fn(async (_sql: string, params?: unknown[]) => {
      const sessionId = params?.[0] as string;
      const roh = metadatenSpeicher.get(sessionId);
      if (!roh) {
        return [{ beantwortet_am: null }];
      }
      const metadaten = JSON.parse(roh) as {
        nachfrageBeantwortetAm?: string;
      };
      return [{ beantwortet_am: metadaten.nachfrageBeantwortetAm ?? null }];
    }),
  },
  executeTransaction: vi.fn(
    async (statements: { sql: string; params: unknown[] }[]) => {
      for (const anweisung of statements) {
        const [beantwortetAm, , sessionId] = anweisung.params as [
          string,
          string,
          string,
        ];
        metadatenSpeicher.set(
          sessionId,
          JSON.stringify({ nachfrageBeantwortetAm: beantwortetAm }),
        );
      }
      return [];
    },
  ),
}));

vi.mock("~/db/write-queue", () => ({
  enqueueDatabaseWrite: (_key: string, write: () => Promise<unknown>) =>
    write(),
}));

import {
  frageranmelden,
  frageVorZusammenfassung,
  markiereNachfrageBeantwortet,
  nachfrageErledigt,
  useNachfrageStore,
} from "./gate";

const ohneTermin = { eventId: null, event: null, bereitsBeantwortet: false };
const mitTermin = {
  eventId: "evt_1",
  event: null,
  bereitsBeantwortet: false,
};

const laden = (stand: unknown) => ({
  standLaden: vi.fn().mockResolvedValue(stand),
});

/** Wartet, bis die Frage im Store steht, ohne echte Zeit zu verbrauchen. */
const bisOffen = async () => {
  for (let i = 0; i < 20 && !useNachfrageStore.getState().offen; i++) {
    await Promise.resolve();
  }
  return useNachfrageStore.getState().offen;
};

describe("frageVorZusammenfassung", () => {
  beforeEach(() => {
    useNachfrageStore.setState({ frager: 0, offen: null });
    vi.restoreAllMocks();
  });

  it("wartet auf die Antwort, wenn ein Dialog montiert ist", async () => {
    frageranmelden();
    const abhaengigkeiten = laden(ohneTermin);

    let fertig = false;
    const lauf = frageVorZusammenfassung("s1", abhaengigkeiten).then(() => {
      fertig = true;
    });

    expect(await bisOffen()).toEqual(
      expect.objectContaining({ sessionId: "s1" }),
    );
    expect(fertig).toBe(false);

    nachfrageErledigt("s1");
    await lauf;

    expect(fertig).toBe(true);
    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  // Der Kern: bei Kalendertermin sind Titel und Teilnehmer schon
  // menschengeschrieben, die Frage waere ueberfluessig.
  it("fragt bei einem Kalendertermin gar nicht erst", async () => {
    frageranmelden();

    await frageVorZusammenfassung("s1", laden(mitTermin));

    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("laeuft ohne montierten Dialog sofort durch und laedt nichts", async () => {
    const abhaengigkeiten = laden(ohneTermin);

    await frageVorZusammenfassung("s1", abhaengigkeiten);

    expect(abhaengigkeiten.standLaden).not.toHaveBeenCalled();
    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("laeuft durch, wenn die Sitzung nicht mehr existiert", async () => {
    frageranmelden();

    await frageVorZusammenfassung("s1", laden(null));

    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("laeuft durch, wenn der Stand nicht lesbar ist", async () => {
    frageranmelden();
    vi.spyOn(console, "error").mockImplementation(() => {});

    await frageVorZusammenfassung("s1", {
      standLaden: vi.fn().mockRejectedValue(new Error("kaputt")),
    });

    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("stellt nie zwei Fragen gleichzeitig", async () => {
    frageranmelden();
    void frageVorZusammenfassung("s1", laden(ohneTermin));
    await bisOffen();

    await frageVorZusammenfassung("s2", laden(ohneTermin));

    expect(useNachfrageStore.getState().offen?.sessionId).toBe("s1");
  });

  // Ohne das haengt die Zusammenfassung fuer immer, wenn das Fenster zugeht.
  it("loest eine offene Frage auf, wenn der letzte Frager sich abmeldet", async () => {
    const abmelden = frageranmelden();

    let fertig = false;
    const lauf = frageVorZusammenfassung("s1", laden(ohneTermin)).then(() => {
      fertig = true;
    });
    await bisOffen();
    expect(fertig).toBe(false);

    abmelden();
    await lauf;

    expect(fertig).toBe(true);
    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("laesst die Frage stehen, solange noch ein Frager da ist", async () => {
    const abmelden = frageranmelden();
    frageranmelden();

    void frageVorZusammenfassung("s1", laden(ohneTermin));
    await bisOffen();

    abmelden();

    expect(useNachfrageStore.getState().offen?.sessionId).toBe("s1");
  });

  it("ignoriert eine Antwort auf eine andere Sitzung", async () => {
    frageranmelden();
    void frageVorZusammenfassung("s1", laden(ohneTermin));
    await bisOffen();

    nachfrageErledigt("s2");

    expect(useNachfrageStore.getState().offen?.sessionId).toBe("s1");
  });
});

/**
 * Einmal pro Gespraech, fuer immer (27.09.2026, siehe `entscheidung.ts`).
 * Anders als oben: hier laeuft `frageVorZusammenfassung` OHNE eigenes
 * `abhaengigkeiten`-Argument, also ueber den echten `echt`-Pfad -- gegen die
 * oben gemockten `~/db` / `~/db/write-queue` / `~/session/content-queries`.
 * Das ist die tatsaechliche Verdrahtung zwischen `markiereNachfrageBeantwortet`
 * (Schreiben) und `echt.standLaden` (Lesen), nicht nur die Entscheidungslogik
 * dahinter.
 */
describe("markiereNachfrageBeantwortet (echte Verdrahtung)", () => {
  beforeEach(() => {
    useNachfrageStore.setState({ frager: 0, offen: null });
    metadatenSpeicher.clear();
  });

  it("fragt nach Skip beim naechsten Stopp derselben Sitzung nicht erneut", async () => {
    frageranmelden();

    let ersterLaufFertig = false;
    const ersterLauf = frageVorZusammenfassung("s1").then(() => {
      ersterLaufFertig = true;
    });
    await bisOffen();
    expect(ersterLaufFertig).toBe(false);

    // Skip: exakt der Weg aus host.tsx `verwerfen`.
    await markiereNachfrageBeantwortet("s1");
    nachfrageErledigt("s1");
    await ersterLauf;
    expect(ersterLaufFertig).toBe(true);

    // Zweiter Stopp derselben Sitzung, z. B. nach "Weiter aufnehmen": faellt
    // sofort durch, ohne zu fragen.
    frageranmelden();
    await frageVorZusammenfassung("s1");
    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("fragt nach erfolgreichem Speichern beim naechsten Stopp derselben Sitzung nicht erneut", async () => {
    frageranmelden();

    let ersterLaufFertig = false;
    const ersterLauf = frageVorZusammenfassung("s2").then(() => {
      ersterLaufFertig = true;
    });
    await bisOffen();

    // Bestaetigen: exakt der Weg aus host.tsx `uebernehmen`s `finally`.
    await markiereNachfrageBeantwortet("s2");
    nachfrageErledigt("s2");
    await ersterLauf;
    expect(ersterLaufFertig).toBe(true);

    frageranmelden();
    await frageVorZusammenfassung("s2");
    expect(useNachfrageStore.getState().offen).toBeNull();
  });

  it("fragt bei einer ANDEREN Sitzung weiterhin wie gewohnt", async () => {
    frageranmelden();
    await markiereNachfrageBeantwortet("s1");

    void frageVorZusammenfassung("s2");

    expect(await bisOffen()).toEqual(
      expect.objectContaining({ sessionId: "s2" }),
    );
  });
});
