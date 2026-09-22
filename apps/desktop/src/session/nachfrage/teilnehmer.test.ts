import { describe, expect, it } from "vitest";

import { fremdeTeilnehmerNamen } from "./teilnehmer";

const mads = { humanId: "u1", source: "manual", name: "Mads Verlin" };
const nina = { humanId: "h2", source: "manual", name: "Nina Falkner" };

describe("fremdeTeilnehmerNamen", () => {
  // Der Kern: der Mensch am Mikrofon ist kein "weiterer Teilnehmer". Gemessen
  // haengt er an allen 9 Sitzungen, in denen die Frage kommt.
  it("laesst den Menschen am Mikrofon weg", () => {
    expect(fremdeTeilnehmerNamen([mads, nina], "u1")).toEqual([
      "Nina Falkner",
    ]);
  });

  it("gibt nichts zurueck, wenn nur der Mensch am Mikrofon dranhaengt", () => {
    expect(fremdeTeilnehmerNamen([mads], "u1")).toEqual([]);
  });

  // Ohne bekannten Besitzer wird nichts abgezogen -- lieber ein Name zu viel
  // als ein fremder Teilnehmer, der stillschweigend verschwindet.
  it("zieht ohne bekannten Besitzer niemanden ab", () => {
    expect(fremdeTeilnehmerNamen([mads, nina], null)).toEqual([
      "Mads Verlin",
      "Nina Falkner",
    ]);
  });

  it("laesst ausdruecklich entfernte Teilnehmer weg", () => {
    expect(
      fremdeTeilnehmerNamen([{ ...nina, source: "excluded" }], "u1"),
    ).toEqual([]);
  });

  it("laesst namenlose Zeilen weg und schneidet Leerzeichen ab", () => {
    expect(
      fremdeTeilnehmerNamen(
        [
          { humanId: "h3", source: "auto", name: "  Anna Beispiel  " },
          { humanId: "h4", source: "auto", name: "   " },
          { humanId: "h5", source: "auto", name: null },
        ],
        "u1",
      ),
    ).toEqual(["Anna Beispiel"]);
  });
});
