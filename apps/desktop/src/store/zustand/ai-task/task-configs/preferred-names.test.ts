import { describe, expect, it } from "vitest";

import {
  appendPreferredNamesGuidance,
  formatPreferredNamesGuidance,
} from "./preferred-names";

const RULES = `Rules for this list:
- Use a spelling from the list only where the material clearly means that exact term. A similar sound alone is not enough.
- Never replace a name in the material with a different entry from this list.
- Never invent a spelling that appears neither in the material you were given nor in the list above.
- If you are unsure which name is meant, keep the spelling from the material or leave the name out.`;

describe("preferred names guidance", () => {
  it("formats dictionary terms as spelling instructions", () => {
    expect(formatPreferredNamesGuidance(["Anarlog", "Char", "anarlog"])).toBe(
      `# Preferred Names

These names and terms are spelled as follows:
- Anarlog
- Char

${RULES}`,
    );
  });

  it("omits empty dictionary lists", () => {
    expect(formatPreferredNamesGuidance([])).toBe("");
    expect(appendPreferredNamesGuidance("Base prompt", [])).toBe("Base prompt");
  });

  it("appends preferred names after the rendered prompt", () => {
    expect(appendPreferredNamesGuidance("Base prompt", ["Anarlog"])).toBe(
      `Base prompt

# Preferred Names

These names and terms are spelled as follows:
- Anarlog

${RULES}`,
    );
  });
});

describe("Namenslizenz eingegrenzt", () => {
  it("erlaubt kein Ueberschreiben abweichender Schreibweisen im Material", () => {
    // Der Satz "even if the transcript or notes spell them differently" hat am
    // 03.09.2026 aus dem Verhoerer "Sedolage" den Kunden "Phonowerk"
    // gemacht, obwohl es um Sedacz ging. Er darf nicht zurueckkommen.
    const guidance = formatPreferredNamesGuidance(["Sedacz", "Phonowerk"]);

    expect(guidance).not.toContain("even if");
    expect(guidance).not.toContain("spell them differently");
  });

  it("verbietet den Tausch gegen einen anderen Listeneintrag", () => {
    const guidance = formatPreferredNamesGuidance(["Sedacz", "Phonowerk"]);

    expect(guidance).toContain("- Sedacz");
    expect(guidance).toContain("- Phonowerk");
    expect(guidance).toContain(
      "Never replace a name in the material with a different entry from this list.",
    );
    expect(guidance).toContain("A similar sound alone is not enough.");
  });

  it("verbietet ausdruecklich eine dritte Schreibweise", () => {
    const guidance = formatPreferredNamesGuidance(["Sedacz"]);

    expect(guidance).toContain(
      "Never invent a spelling that appears neither in the material you were given nor in the list above.",
    );
    expect(guidance).toContain(
      "keep the spelling from the material or leave the name out",
    );
  });

  it("legt einem Modell nie die eingetragene Verhoerung als Wunschwort vor", () => {
    // Ein Alias-Eintrag stand bis zum 03.09.2026 WOERTLICH im Prompt: die
    // Anweisung lautete dann, "Sedacz => Sarec" genau so zu schreiben.
    const guidance = formatPreferredNamesGuidance([
      "Sedacz => Sarec; Saredi",
      "Nordwerk",
    ]);

    expect(guidance).toContain("- Sedacz");
    expect(guidance).toContain("- Nordwerk");
    expect(guidance).not.toContain("=>");
    expect(guidance).not.toContain("Sarec");
    expect(guidance).not.toContain("Saredi");
  });

  it("bleibt leer, wenn nichts eingetragen ist -- auch die Regeln", () => {
    expect(formatPreferredNamesGuidance([])).toBe("");
  });
});
