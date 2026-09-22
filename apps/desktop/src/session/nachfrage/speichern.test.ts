import { beforeEach, describe, expect, it, vi } from "vitest";

import { nachfrageSpeichern, type SpeicherEingabe } from "./speichern";

const basis: SpeicherEingabe = {
  sessionId: "s1",
  ownerUserId: "u1",
  titel: "",
  titelVorher: "",
  neueNamen: [],
  vorhandeneNamen: [],
};

const werkzeuge = () => ({
  titelSetzen: vi.fn().mockResolvedValue(undefined),
  menschAnlegen: vi.fn(async ({ name }: { name: string }) => `h:${name}`),
  teilnehmerAnhaengen: vi.fn().mockResolvedValue(undefined),
});

describe("nachfrageSpeichern", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  // Der eigentliche Zweck: ein gesetzter Titel verhindert, dass der spaetere
  // Titel-Lauf einen erfindet (der laeuft nur bei leerem Feld).
  it("schreibt einen neuen Titel", async () => {
    const w = werkzeuge();

    const ergebnis = await nachfrageSpeichern(
      { ...basis, titel: "  Termin mit Anna  " },
      w,
    );

    expect(w.titelSetzen).toHaveBeenCalledWith("s1", "Termin mit Anna");
    expect(ergebnis.titelGeschrieben).toBe(true);
  });

  it("schreibt nichts, wenn der Titel unveraendert ist", async () => {
    const w = werkzeuge();

    const ergebnis = await nachfrageSpeichern(
      { ...basis, titel: "Jour fixe ", titelVorher: " Jour fixe" },
      w,
    );

    expect(w.titelSetzen).not.toHaveBeenCalled();
    expect(ergebnis.titelGeschrieben).toBe(false);
  });

  // Ein leeres Feld wuerde einen vorhandenen Titel loeschen und dem Modell die
  // Erfindung wieder erlauben.
  it("loescht einen vorhandenen Titel nicht mit einem leeren Feld", async () => {
    const w = werkzeuge();

    await nachfrageSpeichern(
      { ...basis, titel: "   ", titelVorher: "Jour fixe" },
      w,
    );

    expect(w.titelSetzen).not.toHaveBeenCalled();
  });

  it("legt jeden neuen Namen an und haengt ihn an die Sitzung", async () => {
    const w = werkzeuge();

    const ergebnis = await nachfrageSpeichern(
      { ...basis, neueNamen: ["Anna Beispiel", "Bert Muster"] },
      w,
    );

    expect(w.menschAnlegen).toHaveBeenCalledTimes(2);
    expect(w.teilnehmerAnhaengen.mock.calls).toEqual([
      ["s1", "h:Anna Beispiel"],
      ["s1", "h:Bert Muster"],
    ]);
    expect(ergebnis.angelegteNamen).toEqual(["Anna Beispiel", "Bert Muster"]);
  });

  it("legt niemanden doppelt an", async () => {
    const w = werkzeuge();

    const ergebnis = await nachfrageSpeichern(
      {
        ...basis,
        neueNamen: ["  anna beispiel ", "Anna Beispiel", ""],
        vorhandeneNamen: ["Anna Beispiel"],
      },
      w,
    );

    expect(w.menschAnlegen).not.toHaveBeenCalled();
    expect(ergebnis.angelegteNamen).toEqual([]);
  });

  it("legt ohne Besitzer keinen Menschen an", async () => {
    const w = werkzeuge();

    const ergebnis = await nachfrageSpeichern(
      { ...basis, ownerUserId: null, titel: "Neu", neueNamen: ["Anna"] },
      w,
    );

    expect(w.menschAnlegen).not.toHaveBeenCalled();
    expect(ergebnis.titelGeschrieben).toBe(true);
  });

  it("laesst die uebrigen Namen stehen, wenn einer scheitert", async () => {
    const w = werkzeuge();
    w.menschAnlegen.mockImplementationOnce(async () => {
      throw new Error("kaputt");
    });

    const ergebnis = await nachfrageSpeichern(
      { ...basis, neueNamen: ["Anna Beispiel", "Bert Muster"] },
      w,
    );

    expect(ergebnis.angelegteNamen).toEqual(["Bert Muster"]);
  });

  it("schreibt die Namen, auch wenn der Titel scheitert", async () => {
    const w = werkzeuge();
    w.titelSetzen.mockRejectedValueOnce(new Error("kaputt"));

    const ergebnis = await nachfrageSpeichern(
      { ...basis, titel: "Neu", neueNamen: ["Anna Beispiel"] },
      w,
    );

    expect(ergebnis.titelGeschrieben).toBe(false);
    expect(ergebnis.angelegteNamen).toEqual(["Anna Beispiel"]);
  });
});
