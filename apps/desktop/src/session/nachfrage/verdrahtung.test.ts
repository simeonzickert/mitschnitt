import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

/**
 * Ein Waechter ueber der einen Verhaltenszeile, die sonst ungedeckt bliebe.
 *
 * `capture-lifecycle.ts` ist ein React-Hook von ueber 1700 Zeilen mit Ton,
 * Datenbank und Fensterkommunikation; ihn testbar zu schneiden hiesse, genau
 * den Pfad umzubauen, der die Aufnahmen rettet. Das ist keine Arbeit fuer
 * nebenbei.
 *
 * GRENZE DIESES TESTS, ausdruecklich benannt: er prueft den Quelltext, nicht
 * das Verhalten. Er stirbt, wenn der Aufruf verschwindet, sein `await` faellt
 * oder er hinter den Start der Zusammenfassung rutscht. Er laesst durch: ein
 * falsches Argument, einen Zweig, der ihn ueberspringt, und jeden Fehler zur
 * Laufzeit. Wer sich darauf verlaesst, hat nichts geprueft -- den Beweis
 * liefert der Testlauf von Hand.
 */
const quelle = readFileSync(
  join(process.cwd(), "src/stt/capture-lifecycle.ts"),
  "utf8",
);

describe("Verdrahtung im Aufnahme-Lebenszyklus", () => {
  it("wartet die Nachfrage ab", () => {
    expect(quelle).toContain("await frageVorZusammenfassung(sessionId);");
  });

  it("fragt, bevor die Zusammenfassung angestossen wird", () => {
    const frage = quelle.indexOf("await frageVorZusammenfassung(sessionId);");
    const anstoss = quelle.indexOf("service.requestAutoEnhance(");

    expect(frage).toBeGreaterThan(-1);
    expect(anstoss).toBeGreaterThan(-1);
    expect(frage).toBeLessThan(anstoss);
  });

  it("fragt erst, nachdem das Transkript geschrieben ist", () => {
    const transkript = quelle.indexOf(
      "await flushCanonicalSessionEditorChanges(sessionId);",
    );
    const frage = quelle.indexOf("await frageVorZusammenfassung(sessionId);");

    expect(transkript).toBeGreaterThan(-1);
    expect(frage).toBeGreaterThan(transkript);
  });
});
