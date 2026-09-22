import { describe, expect, it } from "vitest";

import {
  addDictionaryAlias,
  dictionaryCanonicalTerms,
  dictionaryEntryKey,
  formatDictionaryEntry,
  isAliasCandidate,
  MAX_ALIAS_WORDS,
  parseAliasInput,
  parseDictionaryEntries,
  parseDictionaryEntry,
  replaceDictionaryEntry,
} from "./dictionary-entry";

describe("Woerterbuch-Zeile lesen", () => {
  it("liest eine Zeile ohne Pfeil als Eintrag ohne Verhoerungen", () => {
    expect(parseDictionaryEntry("Webflow")).toEqual({
      canonical: "Webflow",
      aliases: [],
    });
  });

  it("liest das Aliasformat mit Semikolon", () => {
    expect(
      parseDictionaryEntry("Sedacz => Sedatsch; Sedatz; Seda das"),
    ).toEqual({
      canonical: "Sedacz",
      aliases: ["Sedatsch", "Sedatz", "Seda das"],
    });
  });

  it("wirft leere Stuecke und den Selbst-Alias weg -- wie der Rust-Parser", () => {
    expect(parseDictionaryEntry("Glinck => Glink; ; Kling;")).toEqual({
      canonical: "Glinck",
      aliases: ["Glink", "Kling"],
    });
    expect(parseDictionaryEntry("Noormann => Nohrmann; noormann")).toEqual({
      canonical: "Noormann",
      aliases: ["Nohrmann"],
    });
    expect(parseDictionaryEntry("=> nur Alias")).toBeNull();
    expect(parseDictionaryEntry("   ")).toBeNull();
  });
});

describe("Woerterbuch-Zeile schreiben", () => {
  it("schreibt genau das Format, das der Rust-Parser liest", () => {
    expect(
      formatDictionaryEntry({
        canonical: "Sedacz",
        aliases: ["Sarec", "Saredi"],
      }),
    ).toBe("Sedacz => Sarec; Saredi");
    expect(formatDictionaryEntry({ canonical: "Webflow", aliases: [] })).toBe(
      "Webflow",
    );
  });

  it("laeuft rund: schreiben, lesen, wieder schreiben aendert nichts", () => {
    const line = formatDictionaryEntry({
      canonical: "NOR Drucktechnik",
      aliases: ["NOR Druck Technik", "Nor Drucktechnik"],
    });
    const parsed = parseDictionaryEntry(line);
    expect(parsed).not.toBeNull();
    expect(formatDictionaryEntry(parsed!)).toBe(line);
  });

  // Eine bestehende Liste geht bei jedem Oeffnen der Einstellungen durch
  // parse/format. Die Zeilen unten bilden einen gewachsenen Bestand nach:
  // Selbst-Alias, Dubletten, die sich nur in der Gross-/Kleinschreibung
  // unterscheiden, mehrwortige Verhoerungen. Ein Bestand-Test mit glatten
  // Daten haette genau die fuenf Zeilen verfehlt, die sich wirklich aendern.
  it("aendert an einer gewachsenen Liste nur wirkungslose Dubletten", () => {
    const bestand = [
      "Peak AI => PK; P K; PKI; pk",
      "ClickUp => klick ab; Klick ab; Click ab; klickab",
      "LLMs => lms; LNM; LNMs; L L Ms",
      "Malte Landweg => malte landwek; Malte Landwek",
      "Sedacz => Sedatsch; Sedatz; Seda das; Cedatsch",
      "Nordwerk => Nordwerker; Nord Werk",
      "Phonowerk => Phono Werk; Phono-Werk",
      "Glinck => Glink; Kling",
      "Noormann => Nohrmann; Noormann",
      "NOR Drucktechnik => NOR Druck Technik; Nor Drucktechnik",
    ];

    // Was der Rundlauf streicht, ist AUSSCHLIESSLICH das, was der Rust-Index
    // ohnehin nicht auffinden koennte: der Selbst-Alias und Dubletten, die
    // sich nur in der Gross-/Kleinschreibung unterscheiden (der Index schlaegt
    // kleingeschrieben nach und liefert bei zwei Zielen fuer dieselbe
    // Verhoerung gar nichts). Es geht damit keine Ersetzung verloren.
    expect(parseDictionaryEntries(bestand).map(formatDictionaryEntry)).toEqual([
      "Peak AI => PK; P K; PKI",
      "ClickUp => klick ab; Click ab; klickab",
      "LLMs => lms; LNM; LNMs; L L Ms",
      "Malte Landweg => malte landwek",
      "Sedacz => Sedatsch; Sedatz; Seda das; Cedatsch",
      "Nordwerk => Nordwerker; Nord Werk",
      "Phonowerk => Phono Werk; Phono-Werk",
      "Glinck => Glink; Kling",
      "Noormann => Nohrmann",
      "NOR Drucktechnik => NOR Druck Technik",
    ]);
  });

  it("laesst eine Liste ohne Verhoerungen Zeichen fuer Zeichen stehen", () => {
    const bestand = ["Sedacz", "Nordwerk", "Webflow", "Verlin Media"];
    expect(parseDictionaryEntries(bestand).map(formatDictionaryEntry)).toEqual(
      bestand,
    );
  });

  // Diese drei Zeilen stehen WOERTLICH ein zweites Mal im Rust-Test
  // `was_die_oberflaeche_schreibt_liest_dieser_parser`
  // (crates/vocabulary/src/lib.rs). Zusammen sind sie das Schloss: hier haengt
  // der Schreiber daran, dort der Leser. Wer eine Seite aendert, muss die
  // andere anfassen -- ein stumm nicht mehr wirkender Alias faellt sofort auf.
  it("schreibt genau die Zeilen, gegen die der Rust-Parser geprueft wird", () => {
    expect(
      formatDictionaryEntry({
        canonical: "Sedacz",
        aliases: ["Sarec", "Seda das"],
      }),
    ).toBe("Sedacz => Sarec; Seda das");
    expect(
      addDictionaryAlias(["Nordwerk"], "Nordwerk", "Nordfall"),
    ).toEqual(["Nordwerk => Nordfall"]);
    expect(
      formatDictionaryEntry({
        canonical: "NOR Drucktechnik",
        aliases: ["NOR Druck Technik"],
      }),
    ).toBe("NOR Drucktechnik => NOR Druck Technik");
  });

  it("laesst kein Trennzeichen in den Text, das die Zeile zerschneiden wuerde", () => {
    const line = formatDictionaryEntry({
      canonical: "A; B => C",
      aliases: ["x; y", "p => q"],
    });
    const parsed = parseDictionaryEntry(line);
    expect(parsed?.canonical).toBe("A B C");
    expect(parsed?.aliases).toEqual(["x y", "p q"]);
  });
});

describe("Alias an einen bestehenden Eintrag haengen", () => {
  it("haengt an den vorhandenen kanonischen Eintrag an statt eine Dublette anzulegen", () => {
    expect(
      addDictionaryAlias(["Nordwerk", "Sedacz"], "Sedacz", "Sarec"),
    ).toEqual(["Nordwerk", "Sedacz => Sarec"]);
  });

  it("findet den Eintrag unabhaengig von Gross-/Kleinschreibung und haengt an bestehende Aliasse an", () => {
    expect(
      addDictionaryAlias(["sedacz => Sarec"], "Sedacz", "Saredi"),
    ).toEqual(["sedacz => Sarec; Saredi"]);
  });

  it("legt einen neuen Eintrag an, wenn der Name noch nicht in der Liste steht", () => {
    expect(addDictionaryAlias(["Webflow"], "Glinck", "Kling")).toEqual([
      "Webflow",
      "Glinck => Kling",
    ]);
  });

  it("nimmt denselben Alias nicht zweimal auf -- und gibt die EINGABE zurueck", () => {
    const bestand = ["Sedacz => Sarec"];
    const nachher = addDictionaryAlias(bestand, "Sedacz", "sarec");

    expect(nachher).toEqual(["Sedacz => Sarec"]);
    // Identitaet, nicht nur Gleichheit: der Aufrufer prueft mit `!==`, ob
    // etwas passiert ist. Eine gleiche Kopie waere fuer ihn eine Aenderung.
    expect(nachher).toBe(bestand);
  });
});

describe("Was ueberhaupt ein Alias werden darf", () => {
  it("nimmt kurze Verhoerungen", () => {
    expect(isAliasCandidate("Sarec", "Sedacz")).toBe(true);
    expect(isAliasCandidate("NOR Druck Technik", "NOR Drucktechnik")).toBe(
      true,
    );
  });

  // Die Grenze ist kein Geschmack: sie spiegelt `Options::max_phrase_tokens`
  // aus crates/vocabulary/src/matcher.rs. Der Nachlauf schiebt hoechstens drei
  // Woerter zusammen -- ein vierwortiger Alias koennte nie treffen und waere
  // ein Eintrag, der aussieht, als wuerde er wirken.
  it("laesst genau drei Woerter zu und vier nicht mehr", () => {
    expect(MAX_ALIAS_WORDS).toBe(3);
    expect(isAliasCandidate("NOR Druck Technik", "NOR Drucktechnik")).toBe(
      true,
    );
    expect(isAliasCandidate("NOR Druck Technik GmbH", "NOR Drucktechnik")).toBe(
      false,
    );
    expect(isAliasCandidate("Sarec", "NOR Druck Technik GmbH")).toBe(false);
  });

  it("lehnt ganze Saetze ab -- ein Alias ist ein Wort, keine Korrektur", () => {
    expect(
      isAliasCandidate(
        "wir haben das letzte Woche schon besprochen",
        "Sedacz",
      ),
    ).toBe(false);
    expect(isAliasCandidate("Sarec", "das haben wir so entschieden")).toBe(
      false,
    );
  });

  it("lehnt Leeres und den Selbstbezug ab", () => {
    expect(isAliasCandidate("", "Sedacz")).toBe(false);
    expect(isAliasCandidate("sedacz", "Sedacz")).toBe(false);
  });
});

describe("Alias-Eingabefeld", () => {
  it("nimmt Komma und Semikolon als Trenner", () => {
    expect(parseAliasInput("Sarec, Saredi; Sarrac")).toEqual([
      "Sarec",
      "Saredi",
      "Sarrac",
    ]);
  });

  it("liefert nichts bei leerer Eingabe", () => {
    expect(parseAliasInput("  , ; ")).toEqual([]);
  });
});

describe("Was ein Modell zu sehen bekommt", () => {
  it("gibt nur die richtige Schreibweise heraus, nie die Verhoerung", () => {
    expect(
      dictionaryCanonicalTerms(["Sedacz => Sarec; Saredi", "Webflow"]),
    ).toEqual(["Sedacz", "Webflow"]);
  });

  it("laesst keinen Pfeil durch", () => {
    for (const term of dictionaryCanonicalTerms([
      "Sedacz => Sarec",
      "Nordwerk => Nordfall; Nordwehr",
    ])) {
      expect(term).not.toContain("=>");
    }
  });
});

describe("Befunde der Pruefrunde vom 03.09.2026", () => {
  // C2: `Vocabulary::parse` (crates/vocabulary/src/lib.rs) legt beide Zeilen zu
  // EINEM Eintrag zusammen -- der Nachlauf kennt "Sarec" also laengst. Wer nur
  // bis zur ersten Zeile schaut, haengt ihn ein zweites Mal an und meldet ihn
  // als frisch gelernt.
  it("sieht eine Verhoerung auch in der ZWEITEN Zeile desselben Namens", () => {
    const bestand = ["Sedacz", "sedacz => Sarec"];
    const nachher = addDictionaryAlias(bestand, "Sedacz", "Sarec");

    expect(nachher).toBe(bestand);
  });

  // C3: Rust schneidet vor dem Nachschlagen die Randinterpunktion jedes Wortes
  // ab (`core_of`, crates/vocabulary/src/matcher.rs) und sucht "sarec". Ein
  // gespeichertes "Sarec," traefe nie.
  it("speichert keine Verhoerung mit Randinterpunktion", () => {
    expect(
      formatDictionaryEntry({ canonical: "Sedacz", aliases: ["Sarec,"] }),
    ).toBe("Sedacz => Sarec");
    expect(addDictionaryAlias(["Glinck"], "Glinck", "Kling.")).toEqual([
      "Glinck => Kling",
    ]);
    expect(parseAliasInput("Sarec., (Klick ab)!")).toEqual([
      "Sarec",
      "Klick ab",
    ]);
  });

  it("laesst Bindestrich und Apostroph stehen -- Rust tut es auch", () => {
    expect(
      formatDictionaryEntry({
        canonical: "Tofmann-Gruppe",
        aliases: ["Hof-Mann;", "O'Brien."],
      }),
    ).toBe("Tofmann-Gruppe => Hof-Mann; O'Brien");
  });

  it("wertet ein Paar mit Satzzeichen weiter als Verhoerung", () => {
    expect(isAliasCandidate("Sarec,", "Sedacz")).toBe(true);
    expect(isAliasCandidate(",", "Sedacz")).toBe(false);
  });
});

describe("Einen bestehenden Eintrag bearbeiten", () => {
  // Der Anlass: der Betreiber trug am 03.09.2026 "serredi" als Verhoerung fuer
  // "Sedacz" ein -- aus einer Zusammenfassung abgeschrieben, nicht aus dem
  // Transkript. Das Wort kommt in keiner echten Verhoerung vor, der Eintrag
  // greift nie. Ohne Bearbeiten blieb nur Loeschen + Neuanlegen -- und ein
  // Mensch, der zwei Schritte macht, wo einer reicht, verliert irgendwann den
  // zweiten.
  it("korrigiert einen toten Alias, ohne den Eintrag neu anzulegen", () => {
    const bestand = ["Nordwerk", "Sedacz => serredi"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("Sedacz"),
      { canonical: "Sedacz", aliases: ["Sedatsch", "Sedatz"] },
    );

    expect(nachher).toEqual(["Nordwerk", "Sedacz => Sedatsch; Sedatz"]);
  });

  it("benennt den kanonischen Namen um und behaelt die Position", () => {
    const bestand = ["Webflow", "Glinck => Kling", "Nordwerk"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("Glinck"),
      { canonical: "A-NOR", aliases: ["Kling"] },
    );

    expect(nachher).toEqual(["Webflow", "A-NOR => Kling", "Nordwerk"]);
  });

  // Kollidiert der neue Name mit einem ANDEREN vorhandenen Eintrag, passiert
  // nichts. Zwei Eintraege mit demselben Namen wuerde `Vocabulary::parse`
  // ohnehin zusammenlegen und dabei eine Verhoerungsliste stillschweigend
  // verschmelzen -- kein "bearbeitet", sondern Datenverlust.
  it("verweigert eine Umbenennung, die einen anderen Eintrag ueberschreiben wuerde", () => {
    const bestand = ["Sedacz => Sarec", "Nordwerk => Nordfall"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("Sedacz"),
      { canonical: "Nordwerk", aliases: ["Sarec"] },
    );

    expect(nachher).toBe(bestand);
  });

  // Die Umbenennung auf den EIGENEN Namen (nur Gross-/Kleinschreibung oder
  // Verhoerungen geaendert) ist keine Kollision mit sich selbst.
  it("erlaubt eine Aenderung nur der Gross-/Kleinschreibung", () => {
    const bestand = ["sedacz => Sarec"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("sedacz"),
      { canonical: "Sedacz", aliases: ["Sarec"] },
    );

    expect(nachher).toEqual(["Sedacz => Sarec"]);
  });

  it("laesst die Liste unangetastet, wenn der Name laengst nicht mehr da ist", () => {
    const bestand = ["Nordwerk"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("Sedacz"),
      { canonical: "Sedacz", aliases: ["Sarec"] },
    );

    expect(nachher).toBe(bestand);
  });

  // Zwei rohe Zeilen desselben Namens sind gueltiger Bestand (der Import kann
  // sie erzeugen, siehe "verliert keine Verhoerung..." oben). Das Ersetzen
  // arbeitet auf den ZUSAMMENGELEGTEN Eintraegen -- sonst bliebe die zweite
  // Zeile unter dem alten Namen stehen und der Eintrag waere verdoppelt statt
  // bearbeitet.
  it("legt keinen zweiten Eintrag an, wenn der alte Name auf zwei Zeilen stand", () => {
    const bestand = ["Sedacz => Sarec", "Sedacz => Seda das", "Webflow"];
    const nachher = replaceDictionaryEntry(
      bestand,
      dictionaryEntryKey("Sedacz"),
      { canonical: "Sedacz", aliases: ["Sedatsch"] },
    );

    expect(nachher).toEqual(["Sedacz => Sedatsch", "Webflow"]);
  });
});
