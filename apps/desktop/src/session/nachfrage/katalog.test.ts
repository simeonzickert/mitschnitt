import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Fangt die zwei Klassen Fehler, die keine Oberflaechen-Pruefung finden kann.
 *
 * KLASSE 1 -- der Text steht in keinem Katalog. Reicht man `t` als
 * Funktionsparameter durch, statt es im Bauteil aus `useLingui()` zu holen,
 * findet das lingui-Makro den Text beim Extrahieren nicht. Die Oberflaeche
 * funktioniert und bleibt in jeder Sprache englisch.
 *
 * KLASSE 2 -- der Text steht im Katalog, aber ohne Uebersetzung. Genau das
 * war am 04.09. der englische Kopf "Title" mitten im deutschen Dialog: der
 * Eintrag existierte schon, bevor die Nachfrage-Texte uebersetzt wurden, also
 * erzeugte er beim Uebersetzen keine Diff-Zeile und wurde uebersehen. Der
 * Vorgaenger dieses Tests prueft nur auf `msgid` und war deshalb gruen.
 *
 * Die Oberflaechen-Tests in `dialog.test.tsx` bilden `t` nach und sehen
 * deshalb genau das, was der Code hineingibt -- was daraus im Katalog wird,
 * wissen sie nicht. Das Gate `i18n:check` faengt beides nicht: ein nie
 * extrahierter Text erzeugt keinen Diff, und ein leeres `msgstr` ist fuer
 * lingui ein gueltiger Zustand.
 */
const katalog = readFileSync(
  join(process.cwd(), "src/i18n/locales/de/messages.po"),
  "utf8",
);

/** Liest die deutsche Uebersetzung zu einem msgid, oder null wenn es fehlt. */
function uebersetzung(msgid: string): string | null {
  const zeilen = katalog.split("\n");
  const gesucht = `msgid "${msgid.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
  const treffer = zeilen.indexOf(gesucht);
  if (treffer === -1) {
    return null;
  }
  const msgstr = zeilen[treffer + 1] ?? "";
  const passt = /^msgstr "(.*)"$/.exec(msgstr);
  return passt ? passt[1]! : null;
}

/**
 * Jeder Text, den der Nachfrage-Dialog sichtbar macht -- auch die aus dem
 * uebernommenen Teilnehmer-Block, denn ab jetzt erscheinen sie hier.
 */
const texte = [
  "Who was in this meeting?",
  "Names here are recognized in the transcript.",
  "Title",
  "Only me, continue",
  "Save and summarize",
  "Add participants",
  // Die drei Titelvorschlaege: interpoliert, deshalb mit Platzhaltern.
  "Conversation on {0}",
  "Conversation with {0}",
  "Conversation with {0} and {1} others",
  // Die Zeile "einen neuen Menschen anlegen" aus der Vorschlagsliste.
  'Add "<0>{0}</0>"',
];

describe("Sprachkatalog", () => {
  it.each(texte)("kennt %s", (text) => {
    expect(uebersetzung(text)).not.toBeNull();
  });

  it.each(texte)("uebersetzt %s ins Deutsche", (text) => {
    expect(uebersetzung(text)).not.toBe("");
  });
});
