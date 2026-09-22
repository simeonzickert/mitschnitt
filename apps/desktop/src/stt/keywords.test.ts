import { describe, expect, it } from "vitest";

import { dictionaryEntryKey } from "./dictionary-entry";
import { normalizeKeywordList } from "./keywords";

/**
 * C6 (03.09.2026): Der Vergleichsschluessel muss locale-unabhaengig sein, weil
 * Rusts `to_lowercase` es ist. Im tuerkischen Locale macht
 * `toLocaleLowerCase` aus "I" ein punktloses "ı" -- dieselbe Liste
 * entdoppelte dann nach einer anderen Regel, als der Nachlauf nachschlaegt.
 *
 * Der Test stellt genau dieses Verhalten nach, statt eine Locale zu erwarten,
 * die auf dem Baurechner vielleicht gar nicht installiert ist.
 */
describe("Der Schluessel haengt nicht am Gebietsschema", () => {
  it("entdoppelt IKEA und ikea auch bei tuerkischer Kleinschreibung", () => {
    const echt = String.prototype.toLocaleLowerCase;
    String.prototype.toLocaleLowerCase = function (this: string) {
      // Tuerkisch trifft das GROSSE I: aus "I" wird das punktlose "\u0131",
      // aus "i" bleibt "i". Ein Ersetzen im Ergebnis traefe beide gleich und
      // waere kein Test.
      return echt.call(String(this).replace(/I/g, "\u0131"));
    };
    try {
      expect(normalizeKeywordList(["IKEA", "ikea"])).toEqual(["IKEA"]);
      expect(dictionaryEntryKey("IKEA")).toBe(dictionaryEntryKey("ikea"));
    } finally {
      String.prototype.toLocaleLowerCase = echt;
    }
  });
});
