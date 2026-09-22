/**
 * Die Sammel-Abfrage gegen eine ECHTE Datenbank.
 *
 * Warum es diese Datei gibt: `proposals.test.ts` ersetzt die Bruecke durch
 * einen Mock, und der liefert `context_names_json`, `event_participants_json`
 * und `context_text` fertig ab. Damit sagte kein Test etwas ueber die Abfrage
 * selbst -- die ganze Kalender-Unterabfrage haette geloescht werden koennen,
 * die Trennung von Notiz und Titel ebenfalls, und alles waere gruen geblieben.
 * Ein `expect(sql).toContain("...")` waere keine Abhilfe, sondern dieselbe
 * Attrappe in anderer Form: es prueft die Formulierung, nicht die Wirkung.
 *
 * Was dieser Test NICHT beweist: dass die Spaltennamen zum echten Schema
 * passen. Die Tabellen hier sind von Hand angelegt und tragen nur, was die
 * Abfrage anfasst. Ein umbenanntes Feld faellt erst im laufenden Programm auf.
 * Der Test deckt die LOGIK ab -- Ereigniswahl, eigener Nutzer, leere
 * Kennungen, neuestes Transkript, Trennung von Notiz und Titel.
 */
import { describe, expect, it } from "vitest";

import { SCAN_SESSIONS_SQL } from "./proposals";

/**
 * Ueber `process.getBuiltinModule` geholt, nicht importiert.
 *
 * Ein `import ... from "node:sqlite"` bricht den Bau: der Bundler bekommt die
 * Datei zu sehen und weigert sich, ein eingebautes Node-Modul zu buendeln.
 * Diese Form sieht er nicht -- und sie braucht keinen Eingriff in die
 * Bau-Einstellungen des ganzen Programms fuer einen einzigen Test.
 */
type DatabaseSync = {
  exec(sql: string): void;
  prepare(sql: string): { all(): unknown[]; run(...werte: string[]): unknown };
  close(): void;
};
const { DatabaseSync } = process.getBuiltinModule("node:sqlite") as {
  DatabaseSync: new (pfad: string) => DatabaseSync;
};

// Die Spalten session_id und created_at fehlten in session_documents bis zum
// 03.09.2026 -- und was dieses Schema nicht hat, kann kein Test finden. Der
// Rueckfall der Notiz-Verknuepfung auf session_id war deshalb nicht nur
// ungebaut, sondern strukturell unpruefbar: ein Test haette ihn gar nicht
// schreiben koennen. Zum vierten Mal in dieser Strecke hielt eine ANDERE
// Bedingung einen Test gruen.
//
// Dieser Kommentar steht bewusst HIER und nicht im SQL darunter: die
// Schema-Zeichenkette ist ein Template-Literal, und ein Backtick darin
// beendet es. Genau daran ist der vorige Lauf gescheitert.
const SCHEMA = `
  CREATE TABLE transcripts (
    id TEXT, session_id TEXT, words_json TEXT,
    started_at_ms INTEGER, deleted_at TEXT
  );
  CREATE TABLE sessions (
    id TEXT, title TEXT, event_id TEXT, event_json TEXT, deleted_at TEXT
  );
  CREATE TABLE session_participants (
    session_id TEXT, human_id TEXT, display_name TEXT, email TEXT,
    source TEXT, deleted_at TEXT
  );
  CREATE TABLE humans (id TEXT, name TEXT, email TEXT, deleted_at TEXT);
  -- title fehlte hier bis zum 03.09.2026, und was dieses Schema nicht hat,
  -- kann kein Test finden: dass die Titel-Frage den AUFGELOESTEN Termin gar
  -- nicht ansah, war strukturell unpruefbar.
  CREATE TABLE events (
    id TEXT, participants_json TEXT, tracking_id_event TEXT,
    calendar_id TEXT, title TEXT, started_at TEXT, deleted_at TEXT
  );
  CREATE TABLE session_documents (
    id TEXT, session_id TEXT, kind TEXT, body TEXT,
    created_at TEXT, deleted_at TEXT
  );
`;

type Zeile = Record<string, string>;

/** Legt eine Datenbank an, fuellt sie und fahrt die echte Abfrage darueber. */
function lauf(fuellen: (db: DatabaseSync) => void): Zeile[] {
  const db = new DatabaseSync(":memory:");
  db.exec(SCHEMA);
  db.exec(`
    INSERT INTO sessions (id, title, event_id, event_json, deleted_at)
      VALUES ('s1', 'Vom Modell erfundener Titel', NULL, NULL, NULL);
    INSERT INTO transcripts (id, session_id, words_json, started_at_ms, deleted_at)
      VALUES ('t1', 's1', '[]', 100, NULL);
  `);
  fuellen(db);
  const rows = db.prepare(SCAN_SESSIONS_SQL).all() as unknown as Zeile[];
  db.close();
  return rows;
}

const setzeTermin = (db: DatabaseSync, json: string) =>
  db.prepare("UPDATE sessions SET event_json = ? WHERE id = 's1'").run(json);

describe("die Sammel-Abfrage", () => {
  it("findet den Kalendertermin ueber die Kennung, wenn die Sitzung ihn nicht direkt nennt", () => {
    const [zeile] = lauf((db) => {
      setzeTermin(
        db,
        JSON.stringify({ tracking_id: "tr-1", calendar_id: "cal-1" }),
      );
      db.exec(`
        INSERT INTO events (id, participants_json, tracking_id_event, calendar_id, started_at, deleted_at) VALUES
          ('e1', '[{"name":"Mads Verlin"}]', 'tr-1', 'cal-1', '2026-09-01', NULL);
      `);
    });
    expect(JSON.parse(zeile.event_participants_json)).toEqual([
      { name: "Mads Verlin" },
    ]);
  });

  /**
   * Der Grund fuer das NULLIF.
   *
   * Ohne es werden bei unlesbarem `event_json` beide Seiten zu '', und dann
   * haengt sich JEDES Ereignis an, dessen beide Kennungen leer sind -- eine
   * fremde Teilnehmerliste in einem Gespraech, das gar keinen Termin hat.
   */
  it("haengt kein kennungsloses Ereignis an eine Sitzung ohne lesbaren Termin", () => {
    const [zeile] = lauf((db) => {
      setzeTermin(db, "{kein json");
      db.exec(`
        INSERT INTO events (id, participants_json, tracking_id_event, calendar_id, started_at, deleted_at) VALUES
          ('fremd', '[{"name":"Fremde Person"}]', '', '', '2026-09-01', NULL);
      `);
    });
    expect(zeile.event_participants_json).toBe("[]");
  });

  /**
   * Der ZWEITE Weg zum Termin: die Sitzung nennt ihn direkt.
   *
   * Der Test darueber deckt nur die eingebetteten Kennungen ab
   * (`tracking_id` + `calendar_id`). Der Zweig `event.id = session.event_id`
   * hing bis zum 03.09.2026 an einem Test, der die Titel-Spalte prueft -- und
   * mit deren Ausbau haette er seine einzige Abdeckung verloren. Ein Mutant,
   * der den Zweig entfernt, muss weiterhin sterben, sonst verliert ein
   * Gespraech mit direkt genanntem Termin seine Kalenderteilnehmer.
   *
   * WIRD ROT, wenn der `event_id`-Zweig faellt.
   * LAESST DURCH, wie mehrere Treffer sortiert werden.
   */
  it("findet den Kalendertermin auch, wenn die Sitzung ihn direkt nennt", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO events (id, participants_json, tracking_id_event, calendar_id, title, started_at, deleted_at)
          VALUES ('e1', '[{"name":"Mads Verlin"}]', '', '', 'Nordwerk Anlagenbau', '2026-09-01', NULL);
        UPDATE sessions SET event_id = 'e1' WHERE id = 's1';
      `);
    });
    expect(JSON.parse(zeile.event_participants_json)).toEqual([
      { name: "Mads Verlin" },
    ]);
  });

  /**
   * Ein GELOESCHTER Termin ist kein Termin.
   *
   * Die Loeschpruefung sass schon in der Teilnehmer-Unterabfrage und wanderte
   * mit ihr in den Termin-JOIN; ein Mutant, der sie entfernte, ueberlebte am
   * 03.09.2026 trotzdem jeden Test.
   *
   * Der Test prueft seit dem Ausbau der Beleg-Schicht nur noch die
   * Teilnehmer -- und das ist keine Schwaechung: die Zeile zur Titel-Spalte
   * war die zweite Haelfte einer Zusage, die es nicht mehr gibt, und die
   * verbliebene wird ohne sie genauso rot.
   */
  it("nimmt einen geloeschten Termin nicht fuer die Teilnehmer", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO events (id, participants_json, tracking_id_event, calendar_id, title, started_at, deleted_at)
          VALUES ('e1', '[{"name":"Mads Verlin"}]', '', '', 'Nordwerk Anlagenbau', '2026-09-01', '2026-09-02');
        UPDATE sessions SET event_id = 'e1' WHERE id = 's1';
      `);
    });
    expect(zeile.event_participants_json).toBe("[]");
  });

  it("nimmt die Teilnehmer der Sitzung mit dem gepflegten Namen, nicht dem Anzeigenamen", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO humans VALUES ('h1', 'Mads Verlin', '', NULL);
        INSERT INTO session_participants VALUES ('s1', 'h1', 'mv', '', 'calendar', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Rune Falkner', '', 'manual', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Weg Damit', '', 'excluded', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Auch Weg', '', 'manual', '2026-09-01');
      `);
    });
    expect(JSON.parse(zeile.context_names_json)).toEqual([
      "Mads Verlin",
      "Rune Falkner",
    ]);
  });

  /**
   * Die Mailadresse ist eine EIGENE Namensquelle -- und sie fehlte hier.
   *
   * Sie liefert drueben in Rust ausschliesslich Immunitaet und nie ein Ziel.
   * Ein Teilnehmer ohne gepflegten Namen erreichte Rust damit ueberhaupt nicht,
   * und "Verlin" im Transkript war wieder Freiwild fuer ein aehnlich
   * klingendes "Merlin" aus der Woerterbuchliste. Der Rust-Test dazu blieb
   * gruen, weil er die Adresse von Hand in die Namensliste legte -- etwas, das
   * dieser Lader nie tat. Deshalb steht die Pruefung jetzt hier, wo die Liste
   * wirklich entsteht.
   *
   * Wer einen Namen UND eine Adresse hat, liefert beides: der Vorname schuetzt
   * sonst, der Nachname aus der Adresse nicht.
   */
  it("nimmt auch die Mailadresse der Sitzungsteilnehmer mit", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO humans VALUES ('h1', '', 'mads.verlin@nordwerk.example', NULL);
        INSERT INTO session_participants VALUES ('s1', 'h1', '', '', 'calendar', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Mads', 'mads.falkner@nordwerk.example', 'manual', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Ohne Adresse', '', 'manual', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, '', 'weg@nordwerk.example', 'excluded', NULL);
      `);
    });
    // Namen und Adressen kommen in GETRENNTEN Spalten heraus. Lagen sie in
    // einer, musste Rust sie am "@" auseinanderhalten -- und ein Feld ohne
    // "@" war dann von einem Namen nicht zu unterscheiden.
    expect(JSON.parse(zeile.context_names_json)).toEqual([
      "Mads",
      "Ohne Adresse",
    ]);
    expect(JSON.parse(zeile.context_emails_json)).toEqual([
      "mads.verlin@nordwerk.example",
      "mads.falkner@nordwerk.example",
    ]);
  });

  /**
   * Ein Adressfeld OHNE "@" bleibt eine Adresse.
   *
   * Genau dieser Wert war der ANLASS der Feldtrennung -- und bis zum
   * 03.09.2026 22:30 benutzte kein Ladertest ihn: alle fuehrten Werte mit "@".
   * Damit haetten Namen und Adressen wieder in EINER Spalte zusammenlaufen
   * koennen, ohne dass es hier auffiel; der Fehler wird ja gerade erst
   * sichtbar, wenn sich die Herkunft nicht mehr am Zeichen ablesen laesst.
   */
  it("laesst ein Adressfeld ohne Klammeraffen in der Adressspalte", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_participants VALUES ('s1', NULL, '', 'Verlin', 'calendar', NULL);
      `);
    });
    expect(JSON.parse(zeile.context_names_json)).toEqual([]);
    expect(JSON.parse(zeile.context_emails_json)).toEqual(["Verlin"]);
  });

  it("nimmt je Sitzung nur das neueste Transkript", () => {
    const zeilen = lauf((db) => {
      db.exec(`
        INSERT INTO transcripts VALUES ('t2', 's1', '["neu"]', 200, NULL);
        INSERT INTO transcripts VALUES ('t3', 's1', '["geloescht"]', 300, '2026-09-01');
      `);
    });
    expect(zeilen).toHaveLength(1);
    expect(zeilen[0].words_json).toBe('["neu"]');
  });

  /**
   * Die Notiz kommt EINZELN heraus, nicht an den Kalendertext geklebt.
   *
   * Aus ihr muss erst der Sitzungstitel geschnitten werden, und das geht nur,
   * wenn man sie noch als eigenes Stueck in der Hand hat.
   */
  it("gibt Kalendertext und Notiz getrennt heraus", () => {
    const [zeile] = lauf((db) => {
      setzeTermin(
        db,
        JSON.stringify({
          title: "Termin mit Tofmann",
          description: "zum Angebot",
        }),
      );
      db.exec(`
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('s1', 's1', 'note', 'Der Notiztext', '2026-09-01', NULL);
      `);
    });
    expect(zeile.context_text).toBe("Termin mit Tofmann zum Angebot");
    expect(zeile.note_body).toBe("Der Notiztext");
    expect(zeile.generated_title).toBe("Vom Modell erfundener Titel");
  });

  /**
   * Der Rueckfall auf `session_id`, den der Bestandspfad
   * (`session/content-queries.ts`) laengst hat und diese Abfrage bis zum
   * 03.09.2026 nicht.
   *
   * Eine eingefuehrte Notiz traegt eine eigene Kennung und zeigt ueber
   * `session_id` auf ihr Gespraech. Sie war fuer den Lauf unsichtbar -- und
   * jeder darin von einem MENSCHEN geschriebene Name verlor damit seine
   * Immunitaet.
   *
   * Wird rot, wenn der Rueckfall fehlt.
   * Laesst durch: eine Notiz, die weder ueber die Kennung noch ueber
   * `session_id` zu finden ist -- die gibt es nicht.
   */
  it("findet eine Notiz auch, wenn sie eine eigene Kennung traegt", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('import-17', 's1', 'note', 'Verlin hat zugesagt', '2026-09-01', NULL);
      `);
    });
    expect(zeile.note_body).toBe("Verlin hat zugesagt");
  });

  /**
   * Der erste Zweig ist ABSICHTLICH weiter als der des Bestandspfads: der
   * verlangt zusaetzlich `session_id = session.id`. Eine Notiz mit passender
   * Kennung, aber leerem `session_id` wuerde damit hier verloren gehen, wo
   * sie heute gefunden wird -- ein Rueckfall darf nichts wegnehmen.
   */
  it("verliert eine Notiz unter der Kennung des Gespraechs nicht, wenn ihr session_id leer ist", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('s1', '', 'note', 'Der Notiztext', '2026-09-01', NULL);
      `);
    });
    expect(zeile.note_body).toBe("Der Notiztext");
  });

  /** Gibt es beide, gewinnt die unter der Kennung des Gespraechs. */
  it("nimmt bei zwei Kandidaten die Notiz unter der Kennung des Gespraechs", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('import-17', 's1', 'note', 'Die eingefuehrte', '2026-08-01', NULL);
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('s1', 's1', 'note', 'Die eigene', '2026-09-01', NULL);
      `);
    });
    expect(zeile.note_body).toBe("Die eigene");
  });

  it("nimmt eine geloeschte Notiz auch ueber den Rueckfall nicht", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_documents (id, session_id, kind, body, created_at, deleted_at)
          VALUES ('import-17', 's1', 'note', 'Weg damit', '2026-09-01', '2026-09-02');
      `);
    });
    expect(zeile.note_body).toBe("");
  });

  /**
   * Namen und Adressen kommen aus DERSELBEN Beschreibung, wer ein Teilnehmer
   * ist -- und dieser Test haelt das fest, falls sie je wieder auseinander
   * geschrieben werden.
   *
   * Bis zum 03.09.2026 standen zwei fast gleiche Unterabfragen untereinander,
   * jede mit eigener Kopie der dreiteiligen Bedingung. Die naechste Aenderung
   * an einer davon haette nur eine Haelfte getroffen, und dann liefert die
   * andere weiter Zeilen, die niemand mehr meint. Beide Haelften waren fuer
   * sich richtig -- kein Test haette es gefangen.
   *
   * Wird rot, sobald eine der drei Bedingungen (geloescht, 'excluded', der
   * Join auf `humans`) nur noch fuer eine der beiden Spalten gilt.
   */
  it("haelt Namen und Adressen an dieselben Ausschluesse", () => {
    const [zeile] = lauf((db) => {
      db.exec(`
        INSERT INTO session_participants VALUES ('s1', NULL, 'Bleibt', 'bleibt@nordwerk.example', 'manual', NULL);
        INSERT INTO session_participants VALUES ('s1', NULL, 'Geloescht', 'geloescht@nordwerk.example', 'manual', '2026-09-01');
        INSERT INTO session_participants VALUES ('s1', NULL, 'Ausgeschlossen', 'weg@nordwerk.example', 'excluded', NULL);
        INSERT INTO humans VALUES ('h9', 'Geloeschter Mensch', 'geloeschter@nordwerk.example', '2026-09-01');
        INSERT INTO session_participants VALUES ('s1', 'h9', 'Ersatzname', 'ersatz@nordwerk.example', 'manual', NULL);
      `);
    });
    // Der geloeschte Mensch faellt aus dem Join, also gilt der Anzeigename
    // und die Adresse des Teilnehmers -- fuer BEIDE Spalten gleich.
    expect(JSON.parse(zeile.context_names_json)).toEqual([
      "Bleibt",
      "Ersatzname",
    ]);
    expect(JSON.parse(zeile.context_emails_json)).toEqual([
      "bleibt@nordwerk.example",
      "ersatz@nordwerk.example",
    ]);
  });

  it("uebersteht unlesbares event_json, statt die Zeile zu verlieren", () => {
    const [zeile] = lauf((db) => setzeTermin(db, "{kein json"));
    expect(zeile.session_id).toBe("s1");
    expect(zeile.context_text).toBe("");
  });

  it("laesst geloeschte Sitzungen ganz aus", () => {
    const zeilen = lauf((db) =>
      db.exec("UPDATE sessions SET deleted_at = '2026-09-01' WHERE id = 's1'"),
    );
    expect(zeilen).toEqual([]);
  });
});
