import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  execute: vi.fn(),
  vocabularyProposals: vi.fn(),
}));

vi.mock("~/db", () => ({
  liveQueryClient: { execute: mocks.execute },
}));

vi.mock("@anlg/plugin-local-stt", () => ({
  commands: { vocabularyProposals: mocks.vocabularyProposals },
}));

import {
  fetchProposals,
  loadScanSessions,
  parseDismissed,
  proposalKey,
} from "./proposals";

/** Ein Wort, wie es wirklich in `transcripts.words_json` steht. */
const wort = (text: string, confidence?: number) => ({
  text,
  start_ms: 0,
  end_ms: 0,
  channel: 0,
  // ACHTUNG: `metadata` ist ein JSON-STRING, kein Objekt. Genau das ist die
  // Falle -- wer hier ein Objekt annimmt, liest nie eine Sicherheit und
  // behandelt danach jedes Wort als ungemessen.
  metadata: JSON.stringify({
    timing: { source: "provider_word" },
    ...(confidence === undefined ? {} : { confidence }),
  }),
});

const zeile = (overrides: Record<string, unknown> = {}) => ({
  session_id: "s1",
  words_json: JSON.stringify([wort(" Grandfall", 0.42)]),
  context_names_json: JSON.stringify(["Mads Verlin | nordwerk"]),
  event_participants_json: "[]",
  context_text: "Terminbeschreibung von Hand",
  note_body: "",
  generated_title: "Vom Modell erfundener Titel",
  ...overrides,
});

/** Eine Notiz, so wie der Editor sie speichert. */
const notiz = (bloecke: unknown[]) =>
  JSON.stringify({ type: "doc", content: bloecke });

const ueberschrift = (text: string) => ({
  type: "heading",
  attrs: { level: 1 },
  content: [{ type: "text", text }],
});

const absatz = (text: string) => ({
  type: "paragraph",
  content: [{ type: "text", text }],
});

beforeEach(() => {
  mocks.execute.mockReset();
  mocks.vocabularyProposals.mockReset();
});

describe("loadScanSessions", () => {
  it("liest die gemessene Sicherheit aus dem metadata-STRING", async () => {
    mocks.execute.mockResolvedValue([zeile()]);
    const [session] = await loadScanSessions();
    expect(session.words[0]).toEqual({
      text: " Grandfall",
      measuredConfidence: 0.42,
    });
  });

  /**
   * `null` heisst "nicht gemessen", NICHT "sicher".
   *
   * Gemessen am 03.09.2026 an des Betreibers Datenbank: acht von neun lebenden
   * Transkripten tragen ueberhaupt keinen Wert. Wer die Leerstelle hier zu
   * einer 1,0 macht, macht Netz 2 auf 89 % des Bestands blind -- und zwar
   * lautlos.
   */
  it("macht aus einer fehlenden Sicherheit keine Eins", async () => {
    mocks.execute.mockResolvedValue([
      zeile({ words_json: JSON.stringify([wort(" Grandfall")]) }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.words[0].measuredConfidence).toBeNull();
  });

  it("uebersteht kaputtes JSON, statt den ganzen Lauf zu verlieren", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        words_json: "{kein json",
        context_names_json: "auch nicht",
        event_participants_json: "ebenfalls nicht",
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.words).toEqual([]);
    expect(session.contextNames).toEqual([]);
  });

  /**
   * Der Kalendertermin ist eine EIGENE Namensquelle.
   *
   * Bis zum 03.09.2026 fehlte sie hier, obwohl der Stichwortpfad
   * (`useKeywords.ts`) sie laengst las. Wer nur im Termin steht und nicht in
   * der Teilnehmerliste der Sitzung, erreichte Rust nie -- und der Waechter,
   * der einen echten Namen vor einem aehnlich klingenden
   * Woerterbuch-Eintrag schuetzt, konnte fuer ihn nicht greifen.
   */
  it("nimmt auch die Teilnehmer des Kalendertermins", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        context_names_json: "[]",
        event_participants_json: JSON.stringify([
          { name: "Mads Verlin", is_current_user: false },
          { name: "Mads", is_current_user: true },
        ]),
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.contextNames).toEqual(["Mads Verlin"]);
  });

  /**
   * Die Mailadresse eines Kalendereintrags ist eine eigene Namensquelle.
   *
   * Sie liefert drueben in Rust nur IMMUNITAET und nie ein Ziel. Wer nur mit
   * Adresse eingeladen ist, hatte deshalb bisher gar keinen Schutz; und wer als
   * "Mads" eingeladen ist und "mads.verlin@…" heisst, hatte Schutz fuer den
   * Vornamen und keinen fuer den Nachnamen. Beides schliesst diese Zeile.
   */
  it("nimmt auch die Mailadresse aus dem Kalendertermin", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        context_names_json: "[]",
        event_participants_json: JSON.stringify([
          { name: "", email: "mads.verlin@nordwerk.example" },
          { name: "Mads", email: "mads.falkner@nordwerk.example" },
          { name: "Ohne Adresse" },
          { name: "Mads", email: "weg@x.example", is_current_user: true },
        ]),
      }),
    ]);
    const [session] = await loadScanSessions();
    // Namen und Adressen laufen in GETRENNTEN Feldern zur Rust-Seite. Vorher
    // lagen sie in derselben Liste, und Rust unterschied sie am "@" -- ein
    // Kalenderfeld {email: "Verlin"} OHNE "@" war damit von einem Namen nicht
    // zu trennen und konnte zum Ziel eines Vorschlags werden. Die Herkunft
    // steckt jetzt im Feld, nicht in der Schreibweise des Werts.
    expect(session.contextNames).toEqual(["Mads", "Ohne Adresse"]);
    expect(session.contextEmails).toEqual([
      "mads.verlin@nordwerk.example",
      "mads.falkner@nordwerk.example",
    ]);
  });

  /**
   * Der erzeugte Titel wird GETRENNT gefuehrt.
   *
   * Fuer Gespraeche ohne Kalendertermin schreibt ihn ein Sprachmodell aus dem
   * Transkript. Landete er im belegten Kontext, belegte eine uebernommene
   * Verhoerung sich selbst.
   */
  it("haelt den erzeugten Titel vom belegten Kontext getrennt", async () => {
    mocks.execute.mockResolvedValue([zeile()]);
    const [session] = await loadScanSessions();
    expect(session.contextText).toBe("Terminbeschreibung von Hand");
    expect(session.generatedTitle).toBe("Vom Modell erfundener Titel");
  });

  /**
   * DER BEWUSST ANGENOMMENE PREIS, und er steht hier, damit ihn niemand fuer
   * einen Fehler haelt und "repariert".
   *
   * Die Notiz geht ROH in den Kontext. Traegt sie den erfundenen
   * Sitzungstitel als Ueberschrift, immunisiert dieses eine Wort, und der
   * Lauf verliert dort einen Vorschlag. Der Apparat, der das verhinderte, ist
   * am 03.09.2026 auf Entscheid des Betreibers ausgebaut worden: er schuetzte
   * 3 von 389 anarlog-Sitzungen und 0 von 11 in Mitschnitt und konnte dafuer
   * einen von einem MENSCHEN geschriebenen Absatz loeschen.
   *
   * WIRD ROT, wenn wieder irgendetwas aus der Notiz geschnitten wird.
   * LAESST DURCH, ob die Notiz ueberhaupt gefunden wird -- das steht in
   * `proposals.sql.test.ts`.
   */
  it("laesst den erfundenen Titel in der Notiz stehen, statt sie anzufassen", async () => {
    const notizkoerper = notiz([
      ueberschrift("Grandfall Strategie"),
      absatz("Rune Falkner kommt dazu"),
    ]);
    mocks.execute.mockResolvedValue([
      zeile({
        context_text: "",
        generated_title: "Grandfall Strategie",
        note_body: notizkoerper,
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.contextText).toBe(notizkoerper);
  });

  /**
   * Die Gegenprobe von der anderen Seite: eine Ueberschrift, die NICHT der
   * Sitzungstitel ist, war noch nie ein Kandidat fuers Schneiden. Ohne sie
   * bestuende der Test darueber auch dann, wenn der Kontext pauschal leer
   * bliebe.
   */
  it("behaelt eine Ueberschrift, die nicht der Sitzungstitel ist", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        context_text: "",
        generated_title: "Grandfall Strategie",
        note_body: notiz([
          ueberschrift("Nordwerk Anlagenbau"),
          absatz("Rune Falkner kommt dazu"),
        ]),
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.contextText).toContain("Nordwerk Anlagenbau");
    expect(session.contextText).toContain("Rune Falkner kommt dazu");
  });

  /**
   * DIE ZUSAGE, DIE DEN GANZEN LAUF TRAEGT: der Sitzungstitel ist nie Kontext.
   *
   * "Serredi" war die Erfindung des Modells im Titel eines Gespraechs, in dem
   * es um einen ganz anderen Namen ging. Zaehlte ein solcher Titel als Beleg,
   * immunisierte sich eine uebernommene Verhoerung selbst -- und der Lauf
   * verloere genau den Vorschlag, fuer den er gebaut ist.
   *
   * Kontext UND Notiz sind hier leer, damit der Titel die einzige moegliche
   * Quelle ist. Waere eine der beiden gefuellt, bestuende der Test auch dann,
   * wenn er den Titel gar nicht prueft.
   *
   * WIRD ROT, sobald `generated_title` in `contextText` einflieszt -- egal
   * ueber welche Bedingung.
   * LAESST DURCH, was Rust mit dem Feld `generatedTitle` tut; dort schliesst
   * `known_terms` es aus, und das haelt ein eigener Rust-Test fest.
   */
  it("nimmt einen erfundenen Titel nicht in den Kontext", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        context_text: "",
        generated_title: "Serredi Strategie",
        note_body: "",
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.contextText).not.toContain("Serredi");
    // Und der Beweis, dass er ueberhaupt ankommt: ohne diese Zeile bestuende
    // der Test auch, wenn der Lader die Spalte gar nicht mehr laese.
    expect(session.generatedTitle).toBe("Serredi Strategie");
  });

  /**
   * Der Fund, der den Nachbau der Anzeigelogik gekippt hat.
   *
   * Eine Erwaehnung traegt ihren angezeigten Namen in `attrs.label` und
   * rendert genau den. Der Nachbau las nur `text` und `content` -- eine Notiz,
   * in der "Verlin" sichtbar als Erwaehnung steht, ergab damit LEEREN Kontext,
   * und der Name verlor seine Immunitaet. Seit dem Ausbau geht die Notiz roh
   * durch; der Test haelt die Zusage fest, nicht den Weg dorthin.
   */
  it("verliert den sichtbaren Namen einer Erwaehnung nicht", async () => {
    mocks.execute.mockResolvedValue([
      zeile({
        context_text: "",
        generated_title: "Grandfall Strategie",
        note_body: notiz([
          {
            type: "paragraph",
            content: [
              { type: "mention", attrs: { id: "h1", label: "Verlin" } },
            ],
          },
        ]),
      }),
    ]);
    const [session] = await loadScanSessions();
    expect(session.contextText).toContain("Verlin");
  });
});

// Was die Abfrage SELBST tut -- je Sitzung nur das neueste Transkript, die
// Wahl des Kalendertermins, die Trennung von Notiz und Titel -- steht in
// `proposals.sql.test.ts` und laeuft dort gegen eine echte Datenbank. Hier
// stand dafuer bis zum 03.09.2026 ein `expect(sql).toContain(...)`; das prueft
// die Formulierung und nicht die Wirkung, und es haette jede Umschreibung der
// Abfrage mit gleichem Ergebnis rot gemeldet und jede Loeschung mit anderem
// Ergebnis durchgelassen.

describe("fetchProposals", () => {
  it("reicht Gespraeche, Liste und Verworfenes an Rust weiter", async () => {
    mocks.execute.mockResolvedValue([zeile()]);
    mocks.vocabularyProposals.mockResolvedValue({ status: "ok", data: [] });

    await fetchProposals({
      terms: ["Grandpfeil"],
      dismissed: [proposalKey("Tofmann", "Tochmann")],
    });

    const [sessions, terms, dismissed] =
      mocks.vocabularyProposals.mock.calls[0];
    expect(sessions).toHaveLength(1);
    expect(terms).toEqual(["Grandpfeil"]);
    expect(dismissed).toEqual([{ canonical: "tofmann", alias: "tochmann" }]);
  });

  /**
   * Ein Ausfall darf nie wie ein sauberes Ergebnis aussehen.
   *
   * Ein Fehler, der zu `[]` wird, laesst die Oberflaeche "nichts gefunden"
   * schreiben -- und ein Werkzeug, das schweigt, wenn es kaputt ist, ist
   * schlimmer als eines, das gar nicht laeuft.
   */
  it("wirft bei einem Fehler, statt Leere vorzutaeuschen", async () => {
    mocks.execute.mockResolvedValue([zeile()]);
    mocks.vocabularyProposals.mockResolvedValue({
      status: "error",
      error: "kaputt",
    });
    await expect(fetchProposals({ terms: [], dismissed: [] })).rejects.toThrow(
      "kaputt",
    );
  });

  /** Ohne Transkripte gar nicht erst ueber die Bruecke gehen. */
  it("fragt Rust nicht, wenn es nichts zu lesen gibt", async () => {
    mocks.execute.mockResolvedValue([]);
    expect(
      await fetchProposals({ terms: ["Grandpfeil"], dismissed: [] }),
    ).toEqual([]);
    expect(mocks.vocabularyProposals).not.toHaveBeenCalled();
  });
});

describe("proposalKey", () => {
  /**
   * Dieselbe Gleichheit wie im Woerterbuch. Sonst kommt ein abgelehntes
   * "Grandfall" als "grandfall" wieder -- und Ablehnen waere nicht dauerhaft.
   */
  it("vergleicht ohne Ruecksicht auf Gross- und Kleinschreibung", () => {
    expect(proposalKey("Grandpfeil", "Grandfall")).toBe(
      proposalKey("grandpfeil", "GRANDFALL"),
    );
  });

  /**
   * Die Gegenseite zu `key()` in `crates/vocabulary/src/proposals.rs`.
   *
   * Dort steht ein Sonderfall fuer `U+FEFF`, weil JavaScripts `\s` das Zeichen
   * mit abdeckt und Rusts `char::is_whitespace` nicht. Bisher hielt nur ein
   * Rust-Test die eine Haelfte fest -- ein Test, der die ANDERE Seite prueft,
   * fehlte, und genau der wuerde rot, wenn jemand hier `\s` gegen etwas
   * Engeres tauscht. Dann bekaeme ein aus einer Tabelle kopierter Begriff zwei
   * verschiedene Schluessel, und ein verworfenes Paar kaeme zurueck.
   */
  it("behandelt ein eingeschlepptes Byte-Reihenfolge-Zeichen wie Rust", () => {
    expect(proposalKey("Grand\uFEFFpfeil", "x")).toBe(
      proposalKey("Grand pfeil", "x"),
    );
    expect(proposalKey("\uFEFFGrandpfeil", "x")).toBe(
      proposalKey("Grandpfeil", "x"),
    );
  });

  it("liest zurueck, was es geschrieben hat", () => {
    expect(parseDismissed([proposalKey("Tofmann", "Tochmann")])).toEqual([
      { canonical: "tofmann", alias: "tochmann" },
    ]);
  });

  it("laesst kaputte Eintraege liegen, statt Unsinn zu bauen", () => {
    expect(parseDismissed(["", "nurEins"])).toEqual([]);
  });

  /**
   * Der Grund, warum das Trennzeichen kein Leerzeichen sein darf.
   *
   * des Betreibers Liste enthaelt "Nordwerk Anlagenbau" und "Rune Falkner". An einem
   * Leerzeichen zerbraeche der Name in der Mitte -- das verworfene Paar kaeme
   * beim naechsten Lauf zurueck, und Ablehnen waere nicht dauerhaft.
   */
  it("uebersteht einen Namen mit Leerzeichen", () => {
    expect(
      parseDismissed([
        proposalKey("Nordwerk Anlagenbau", "Nordwerk Anlagen Bau"),
      ]),
    ).toEqual([
      { canonical: "nordwerk anlagenbau", alias: "nordwerk anlagen bau" },
    ]);
  });
});

// Hier stand bis zum 03.09.2026 eine Pruefung von `mergeStrings`. Sie taugte
// nichts: sie reichte dem zweiten Aufruf das Ergebnis des ersten von Hand
// herein -- genau das, was die echte Klick-Behandlung NICHT tat. Der Mutant
// "der Behandler ignoriert den Merker" ueberlebte sie deshalb. Was an ihre
// Stelle tritt, steht in `settings/dictionary/index.test.tsx` und klickt
// wirklich.
