import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  frageranmelden,
  frageVorZusammenfassung,
  nachfrageErledigt,
  useNachfrageStore,
} from "./gate";

const ohneTermin = { eventId: null, event: null };
const mitTermin = { eventId: "evt_1", event: null };

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
