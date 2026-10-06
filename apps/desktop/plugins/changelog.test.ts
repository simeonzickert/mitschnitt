import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { changelogSectionToAppMarkdown } from "./changelog";

// Eingebettetes Beispiel-Changelog: drei Versionen, eine mit Note. Die
// Reihenfolge ist absichtlich absteigend wie im echten CHANGELOG.md, und
// 0.1.1 steht direkt neben 0.1.10 -- genau die Verwechslung, die eine
// Regex ueber die Versionsnummer (Punkt als Sonderzeichen) verursachen
// wuerde, soll hier auffallen.
const BEISPIEL = `# Changelog

## 0.1.10 (2026-09-29)

### Note

Bitte einmal von Hand installieren.

### What's new

- **Fixed: Mac updates install again.** The package is clean now.
- Zweiter Stichpunkt.

## 0.1.2 (2026-09-28)

### What's new

- Nur ein Punkt ohne Note.

## 0.1.1 (2026-09-27)

### What's new

- Erster Punkt mit [Link](https://example.com) und **Fett**.
`;

describe("changelogSectionToAppMarkdown", () => {
  it("waehlt den Abschnitt der exakten Version, nicht 0.1.1 statt 0.1.10", () => {
    const result = changelogSectionToAppMarkdown(BEISPIEL, "0.1.10");
    expect(result).not.toBeNull();
    expect(result).toContain("Fixed: Mac updates install again.");
    expect(result).not.toContain("Nur ein Punkt ohne Note.");
    expect(result).not.toContain("Erster Punkt mit");
  });

  it("waehlt 0.1.1 und nicht 0.1.10", () => {
    const result = changelogSectionToAppMarkdown(BEISPIEL, "0.1.1");
    expect(result).not.toBeNull();
    expect(result).toContain("Erster Punkt mit");
    expect(result).not.toContain("Fixed: Mac updates install again.");
  });

  it("uebernimmt das Datum aus der Kopfzeile", () => {
    const result = changelogSectionToAppMarkdown(BEISPIEL, "0.1.10");
    expect(result).toContain('date: "2026-09-29"');
  });

  it("baut die summary ohne Markdown, einzeilig", () => {
    const result = changelogSectionToAppMarkdown(BEISPIEL, "0.1.1");
    expect(result).toContain('summary: "Erster Punkt mit Link und Fett."');
    // Keine Markdown-Reste in der summary-Zeile.
    const summaryLine = result!
      .split("\n")
      .find((line: string) => line.startsWith("summary: "));
    expect(summaryLine).toBeDefined();
    expect(summaryLine).not.toContain("**");
    expect(summaryLine).not.toContain("[");
    expect(summaryLine).not.toContain("](");
  });

  it("setzt den Banner nur, wenn eine Note vorhanden ist", () => {
    const mitNote = changelogSectionToAppMarkdown(BEISPIEL, "0.1.10");
    expect(mitNote).toContain('<banner title="Note" variant="info">');
    expect(mitNote).toContain("Bitte einmal von Hand installieren.");
    expect(mitNote).toContain("</banner>");

    const ohneNote = changelogSectionToAppMarkdown(BEISPIEL, "0.1.2");
    expect(ohneNote).not.toBeNull();
    expect(ohneNote).not.toContain("<banner");
  });

  it("gibt null zurueck, wenn die Version fehlt", () => {
    expect(changelogSectionToAppMarkdown(BEISPIEL, "0.1.99")).toBeNull();
  });

  it("gibt null zurueck, wenn What's new leer ist", () => {
    const leer = `# Changelog

## 0.2.0 (2026-10-01)

### Note

Nur eine Note, kein What's new.
`;
    expect(changelogSectionToAppMarkdown(leer, "0.2.0")).toBeNull();
  });

  it("gibt null zurueck, wenn What's new fehlt", () => {
    const ohne = `# Changelog

## 0.2.0 (2026-10-01)

### Note

Nur eine Note.
`;
    expect(changelogSectionToAppMarkdown(ohne, "0.2.0")).toBeNull();
  });
});

// Test gegen das echte CHANGELOG.md. Geprueft wird die NEUESTE Version im
// Changelog (erster "## "-Abschnitt), nicht die Version aus package.json:
// Dev-Baeute zaehlen die Patch-Version automatisch hoch und haetten sonst
// keinen Abschnitt -- der Test wuerde dann scheitern, obwohl der Changelog
// in Ordnung ist. So bricht jeder Bau, dessen neuester Changelog-Eintrag
// kein "What's new" hat, schon hier.
describe("CHANGELOG.md im Repo-Root", () => {
  const changelogPath = resolve(__dirname, "../../../CHANGELOG.md");

  function neuesteVersion(changelogMd: string): string | null {
    for (const line of changelogMd.split("\n")) {
      if (!line.startsWith("## ")) continue;
      const rest = line.slice(3);
      const parenIndex = rest.indexOf(" (");
      return parenIndex > 0 ? rest.slice(0, parenIndex) : rest;
    }
    return null;
  }

  it("hat fuer die neueste Version einen nicht-leeren What's-new-Abschnitt", () => {
    const changelogMd = readFileSync(changelogPath, "utf-8");
    const version = neuesteVersion(changelogMd);
    expect(version).not.toBeNull();

    const result = changelogSectionToAppMarkdown(changelogMd, version!);
    expect(result).not.toBeNull();
    expect(result!.length).toBeGreaterThan(0);
    // Der Inhalt muss den What's-new-Koerper enthalten, nicht nur Frontmatter.
    expect(result).toMatch(/^- /m);
  });
});
