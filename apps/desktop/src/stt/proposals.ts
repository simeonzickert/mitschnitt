/**
 * Vorschlaege fuer fehlende Woerterbuch-Eintraege.
 *
 * Diese Datei sammelt das Material und reicht es an Rust
 * (`anlg_vocabulary::proposals`) weiter. Die Entscheidung, was ein Vorschlag
 * ist, faellt dort -- hier steht keine zweite Fassung der Regeln, aus
 * demselben Grund, aus dem die 962-zeilige Stoppwort-Kopie am 03.09.2026
 * geloescht wurde: zwei Fassungen derselben Regel driften, und die Oberflaeche
 * wird dann still zur Luege.
 *
 * Was hier NICHT passiert: nichts wird ersetzt und nichts wird eingetragen.
 * Angenommen wird ein Vorschlag erst durch einen Menschen, und dann ueber
 * denselben Weg wie jede Korrektur (`addDictionaryAlias`).
 */

import { useQuery } from "@tanstack/react-query";

import {
  commands as localSttCommands,
  type VocabularyProposal,
} from "@anlg/plugin-local-stt";

import { dictionaryEntryKey } from "./dictionary-entry";

import { liveQueryClient } from "~/db";

export type { VocabularyProposal };

/**
 * Der Schluessel, unter dem ein verworfenes Paar gemerkt wird.
 *
 * Dieselbe Gleichheit wie im Woerterbuch (`dictionaryEntryKey`), damit ein
 * abgelehntes "Grandfall" nicht als "brandfall" wiederkommt.
 */
export const proposalKey = (canonical: string, alias: string): string =>
  `${dictionaryEntryKey(canonical)}${DISMISS_SEPARATOR}${dictionaryEntryKey(alias)}`;

/**
 * Das Trennzeichen zwischen den beiden Haelften eines verworfenen Paares.
 *
 * Ein Leerzeichen waere falsch, und zwar nicht theoretisch: des Betreibers Liste
 * enthaelt "Nordwerk Anlagenbau" und "Rune Falkner". An einem Leerzeichen
 * zerbraeche der Name in der Mitte, und das verworfene Paar kaeme beim
 * naechsten Lauf zurueck. `\u0000` kann in keinem getippten Begriff stehen;
 * dasselbe Trennzeichen benutzt `risky-aliases.ts` fuer denselben Zweck.
 *
 * ALS FLUCHTFOLGE geschrieben, nie als rohes Zeichen. Ein rohes Null-Byte
 * macht die Datei fuer git binaer -- sie faellt danach lautlos aus jedem
 * Pruef-Diff, und kein Mensch sieht ihren Inhalt je wieder. Genau das ist
 * dieser Datei am 03.09.2026 passiert.
 */
const DISMISS_SEPARATOR = "\u0000";

export function parseDismissed(
  raw: string[],
): { canonical: string; alias: string }[] {
  return raw.flatMap((entry) => {
    const [canonical, alias] = entry.split(DISMISS_SEPARATOR);
    return canonical && alias ? [{ canonical, alias }] : [];
  });
}

type TranscriptRow = {
  session_id: string;
  words_json: string;
  context_names_json: string;
  context_emails_json: string;
  event_participants_json: string;
  context_text: string;
  note_body: string;
  generated_title: string;
};

/**
 * Ein Wort, so wie es in `transcripts.words_json` steht.
 *
 * `metadata` ist dort ein JSON-STRING, kein Objekt -- die gemessene
 * Wortsicherheit steckt eine Ebene tiefer, als der Spaltenname vermuten
 * laesst.
 */
type StoredWord = {
  text?: string | null;
  metadata?: string | Record<string, unknown> | null;
};

const measuredConfidenceOf = (word: StoredWord): number | null => {
  const metadata = word.metadata;
  const parsed =
    typeof metadata === "string" && metadata.length > 0
      ? safeParse(metadata)
      : typeof metadata === "object" && metadata !== null
        ? metadata
        : null;
  const value = (parsed as { confidence?: unknown } | null)?.confidence;
  // Fehlt der Wert, bleibt es `null` -- "nicht gemessen", nicht "sicher".
  return typeof value === "number" ? value : null;
};

const safeParse = (raw: string): unknown => {
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
};

const parseStringList = (raw: string): string[] => {
  const parsed = safeParse(raw);
  return Array.isArray(parsed)
    ? parsed.filter((entry): entry is string => typeof entry === "string")
    : [];
};

/**
 * Die Namen UND Mailadressen aus der Teilnehmerliste des Kalendertermins.
 *
 * Der EIGENE Eintrag faellt raus -- dieselbe Regel wie in
 * `useKeywords.ts:parseEventParticipantNames`. Der Betreiber spricht in jedem
 * Gespraech; sein Name als Anker brauchte diese Quelle nicht.
 *
 * Die Adresse kommt ZUSAETZLICH mit, nicht als Ersatz -- und in einem EIGENEN
 * Feld. Rust macht daraus nur IMMUNITAET, nie ein Ziel (`context_emails` geht
 * ausschliesslich in `known_terms`). Bis zum 03.09.2026 lagen Name und Adresse
 * in derselben Liste und Rust unterschied sie am "@" -- eine Adresse ohne "@"
 * war damit ein Ziel wie jeder Name. Die Herkunft steht jetzt im Feld statt im
 * Wert, und dann kann sie nicht mehr verlorengehen. Sie schliesst zwei
 * Loecher:
 *
 * - Wer nur mit Adresse eingeladen ist, hatte gar keinen Schutz.
 * - Wer als "Mads" eingeladen ist und "mads.verlin@nordwerk.example" heisst, hatte
 *   Schutz fuer den Vornamen und keinen fuer den Nachnamen -- obwohl der
 *   Nachname im Transkript steht und genau er das Opfer eines aehnlich
 *   klingenden Woerterbuch-Eintrags wird.
 *
 * GEMESSEN am 03.09.2026 an der laufenden Datenbank: von 330 Kalendereintraegen
 * traegt heute jeder einen Namen, und kein einziger Sitzungsteilnehmer ist
 * namenlos. Das Loch ist also strukturell, nicht beobachtet -- es kostet aber
 * nichts, und die Fehlerrichtung ist die billige: mehr Immunitaet heisst
 * hoechstens ein Vorschlag weniger, weniger Immunitaet heisst ein geopferter
 * Name.
 */
const eventParticipantFields = (
  raw: string,
): { names: string[]; emails: string[] } => {
  const parsed = safeParse(raw);
  const names: string[] = [];
  const emails: string[] = [];
  if (!Array.isArray(parsed)) {
    return { names, emails };
  }
  for (const participant of parsed) {
    if (
      (participant as { is_current_user?: unknown })?.is_current_user === true
    ) {
      continue;
    }
    const felder = participant as { name?: unknown; email?: unknown };
    if (typeof felder.name === "string" && felder.name.trim().length > 0) {
      names.push(felder.name.trim());
    }
    // Was im Adressfeld steht, bleibt eine Adresse -- auch wenn es wie ein
    // Name aussieht. Bis zum 03.09.2026 landeten beide in derselben Liste,
    // und ein Feld {email: "Verlin"} ohne "@" war drueben von einem Namen
    // nicht mehr zu unterscheiden: aus einer Quelle, die nur Immunitaet
    // liefern darf, wurde ein Ersetzungsziel.
    if (typeof felder.email === "string" && felder.email.trim().length > 0) {
      emails.push(felder.email.trim());
    }
  }
  return { names, emails };
};

/**
 * Die Gespraeche mit ihrem Kontext.
 *
 * Die Kontextquellen sind absichtlich DIESELBEN, die auch die Stichwortliste
 * fuer den Erkenner speisen (`getSessionKeywords` in `useKeywords.ts`):
 * Sitzungsteilnehmer, KALENDERTEILNEHMER, Kalendertitel,
 * Kalenderbeschreibung, Notiz. Was dem Erkenner als richtig vorgelegt wird,
 * ist auch hier der Massstab.
 *
 * Die Kalenderteilnehmer sind kein Beiwerk: bis zum 03.09.2026 fehlten sie
 * hier, obwohl der Stichwortpfad sie laengst las. Wer nur im Kalendertermin
 * steht und nicht in der Teilnehmerliste der Sitzung, erreichte Rust nie --
 * und der Waechter, der einen echten Namen davor schuetzt, einem
 * aehnlich klingenden Woerterbuch-Eintrag geopfert zu werden, konnte fuer ihn
 * nicht greifen. Das war derselbe Fehler wie der urspruengliche, nur in
 * anderer Verkleidung.
 *
 * Der Sitzungstitel liegt bewusst in einem EIGENEN Feld: fuer Gespraeche ohne
 * Kalendertermin schreibt ihn ein Sprachmodell aus dem Transkript, er ist
 * also zirkulaer und belegt nichts. Rust nimmt ihn deshalb nicht als Beleg.
 *
 * Je Sitzung wird nur das NEUESTE Transkript genommen. Die Datenbank trug am
 * 03.09.2026 sechzehn Transkripte zu neun Sitzungen -- mehrfach neu
 * gerechnete Aufnahmen. Zaehlte man sie alle, saehe Netz 1 dieselbe
 * Schreibweise mehrfach und hielte eine einzige Aufnahme fuer ein Muster ueber
 * mehrere Gespraeche.
 *
 * Als Konstante herausgegeben, damit ein Test sie gegen eine ECHTE Datenbank
 * fahren kann.
 *
 * Solange die Tests nur den Mock der Bruecke sahen, lieferte dieser die
 * Spalten fertig -- die ganze Kalender-Unterabfrage haette entfernt werden
 * koennen und kein Test waere rot geworden. `proposals.sql.test.ts` legt jetzt
 * ein Schema an, schreibt Zeilen hinein und liest das Ergebnis; damit prueft
 * er die Abfrage und nicht ihre Formulierung.
 */
export const SCAN_SESSIONS_SQL = `
    -- EINE Beschreibung, wer ein Teilnehmer dieses Gespraechs ist.
    --
    -- Bis zum 03.09.2026 standen hier zwei fast gleiche Unterabfragen
    -- untereinander -- eine fuer den Namen, eine fuer die Adresse -- mit
    -- demselben Join und derselben dreiteiligen Bedingung, von Hand doppelt
    -- geschrieben. Die naechste Aenderung an einer der drei (der
    -- Geloescht-Pruefung, dem 'excluded'-Ausschluss, dem Join auf humans)
    -- haette nur eine Haelfte getroffen, und dann liefert die andere weiter
    -- Zeilen, die niemand mehr meint. Kein Test haette das gefangen: beide
    -- Haelften waren fuer sich richtig.
    WITH teilnehmer AS (
      SELECT
        participant.session_id AS session_id,
        COALESCE(NULLIF(human.name, ''), participant.display_name) AS name,
        COALESCE(NULLIF(human.email, ''), participant.email) AS email
      FROM session_participants AS participant
      LEFT JOIN humans AS human
        ON human.id = participant.human_id
        AND human.deleted_at IS NULL
      WHERE participant.source <> 'excluded'
        AND participant.deleted_at IS NULL
    )
    SELECT
      transcript.session_id,
      transcript.words_json,
      COALESCE((
        SELECT json_group_array(name)
        FROM teilnehmer
        WHERE teilnehmer.session_id = session.id
          AND teilnehmer.name <> ''
      ), '[]') AS context_names_json,
      -- Die Adressen GETRENNT, und das ist keine Kosmetik.
      --
      -- Sie liefern drueben in Rust ausschliesslich IMMUNITAET und nie ein
      -- Ziel. Bis zum 03.09.2026 wurden sie hier mit den Namen in EINE Liste
      -- geworfen, und Rust unterschied die beiden Rollen daran, ob ein Wert
      -- ein "@" traegt. Ein Adressfeld mit dem Wert "Verlin" -- ohne "@" --
      -- war danach von einem Namen nicht mehr zu unterscheiden und wurde zum
      -- ZIEL: der Lauf haette vorgeschlagen, kuenftig "Werlin" durch "Verlin"
      -- zu ersetzen, auf Grundlage einer Quelle, die genau das nie tun darf.
      -- Die Herkunft ging beim Zusammenwerfen verloren, also wird sie jetzt
      -- mitgetragen statt hinterher geraten.
      --
      -- Ein Teilnehmer ohne gepflegten Namen erreichte Rust bis zum
      -- 03.09.2026 ueberhaupt nicht -- der Waechter, der einen echten Namen
      -- davor schuetzt, einem aehnlich klingenden Woerterbuch-Eintrag
      -- geopfert zu werden, konnte fuer ihn nicht greifen. Ein Rust-Test
      -- hielt den Fall fuer gedeckt, weil er die Adresse von Hand in die
      -- Namensliste legte -- etwas, das dieser Lader nie tat.
      COALESCE((
        SELECT json_group_array(email)
        FROM teilnehmer
        WHERE teilnehmer.session_id = session.id
          AND teilnehmer.email <> ''
      ), '[]') AS context_emails_json,
      COALESCE(termin.participants_json, '[]') AS event_participants_json,
      TRIM(
        CASE
          WHEN json_valid(session.event_json)
          THEN COALESCE(json_extract(session.event_json, '$.title'), '') || ' ' ||
               COALESCE(json_extract(session.event_json, '$.description'), '')
          ELSE ''
        END
      ) AS context_text,
      -- Die Notiz kommt ROH heraus und geht ROH weiter.
      --
      -- Bis zum 03.09.2026 wurde hier der Sitzungstitel herausgeschnitten, der
      -- als Ueberschrift im gespeicherten Dokument steht. Der Apparat dafuer
      -- ist AUSGEBAUT, auf Entscheid des Betreibers, und die Begruendung
      -- gehoert hierher, damit ihn niemand aus guter Absicht wieder
      -- hinschreibt: er schuetzte 3 von 389 anarlog-Sitzungen und 0 von 11 in
      -- Mitschnitt, und er hat in vier aufeinanderfolgenden Pruefrunden
      -- Blocker erzeugt -- zuletzt zwei, bei denen er einen von einem
      -- MENSCHEN geschriebenen Absatz geloescht haette. Ein Mechanismus, der
      -- menschlichen Text loeschen kann, ist den Randfall nicht wert.
      --
      -- Der Preis ist bekannt und angenommen: in den drei Sitzungen, deren
      -- Notiz den erfundenen Titel als Ueberschrift traegt, immunisiert dieses
      -- eine Wort, und dort faellt ein Vorschlag aus.
      COALESCE(note.body, '') AS note_body,
      COALESCE(session.title, '') AS generated_title
    FROM transcripts AS transcript
    JOIN sessions AS session
      ON session.id = transcript.session_id
      AND session.deleted_at IS NULL
    -- EINE Aufloesung des Kalendertermins, zwei Verbraucher: die Teilnehmer
    -- und die Frage, ob der Sitzungstitel ein Beleg ist.
    --
    -- Vorher stand die Aufloesung nur in der Teilnehmer-Unterabfrage, und die
    -- Titel-Frage hatte ihre eigene, engere Fassung. Zwei Begriffe von
    -- "dieses Gespraech hat einen Termin", die auseinanderliefen -- genau die
    -- Klasse, die weiter oben schon einmal zwei fast gleiche Unterabfragen
    -- erzeugt hatte. Wer die Bedingung jetzt aendert, aendert beide Seiten.
    --
    -- Als JOIN auf die aufgeloeste Kennung, nicht als zwei Unterabfragen --
    -- dasselbe Muster, das die Notiz darunter schon benutzt.
    LEFT JOIN events AS termin
      ON termin.id = (
        SELECT event.id
        FROM events AS event
        WHERE event.deleted_at IS NULL
          AND (
            event.id = session.event_id
            -- NULLIF ist der Unterschied zwischen "kein Termin" und "jeder
            -- Termin ohne Kennung". Ohne es werden beide Seiten bei
            -- unlesbarem event_json zu '', und dann haengt sich jedes Ereignis
            -- an, dessen beide Kennungen leer sind. Mit NULL vergleicht SQL
            -- nie wahr -- die sichere Fehlerrichtung ist "kein Treffer".
            OR (
              event.tracking_id_event = NULLIF(CASE
                WHEN json_valid(session.event_json)
                THEN json_extract(session.event_json, '$.tracking_id')
                ELSE ''
              END, '')
              AND event.calendar_id = NULLIF(CASE
                WHEN json_valid(session.event_json)
                THEN json_extract(session.event_json, '$.calendar_id')
                ELSE ''
              END, '')
            )
          )
        ORDER BY event.started_at, event.id
        LIMIT 1
      )
    -- Die Notiz haengt normalerweise unter DERSELBEN Kennung wie das
    -- Gespraech. "Normalerweise" ist hier das Problem: der Bestandspfad
    -- (session/content-queries.ts) faellt zusaetzlich auf die Spalte
    -- session_id zurueck, dieser hier tat es bis zum 03.09.2026 nicht. Eine
    -- eingefuehrte Notiz mit eigener Kennung -- {id: "import-17", session_id:
    -- "s1"} -- war fuer den Lauf unsichtbar, und jeder darin von einem
    -- MENSCHEN geschriebene Name verlor seine Immunitaet: aus einem richtig
    -- getippten "Verlin" wurde die angebliche Verhoerung eines aehnlich
    -- klingenden Woerterbuch-Eintrags.
    --
    -- GEMESSEN am 03.09.2026 an der laufenden Datenbank: 13 Notizen, alle
    -- unter der Kennung des Gespraechs, null Faelle. Das Loch ist also
    -- strukturell, nicht beobachtet -- und die Fehlerrichtung die billige:
    -- eine Notiz mehr zu lesen kostet hoechstens einen Vorschlag, eine
    -- weniger kostet einen echten Namen.
    --
    -- Der erste Zweig ist ABSICHTLICH weiter als der des Bestandspfads: der
    -- verlangt zusaetzlich session_id = session.id, und eine Notiz mit
    -- passender Kennung aber leerem session_id wuerde damit hier verloren
    -- gehen, wo sie heute gefunden wird. Ein Rueckfall darf nichts wegnehmen.
    LEFT JOIN session_documents AS note
      ON note.id = COALESCE(
        (
          SELECT direkt.id
          FROM session_documents AS direkt
          WHERE direkt.id = session.id
            AND direkt.kind = 'note'
            AND direkt.deleted_at IS NULL
          LIMIT 1
        ),
        (
          SELECT rueckfall.id
          FROM session_documents AS rueckfall
          WHERE rueckfall.session_id = session.id
            AND rueckfall.kind = 'note'
            AND rueckfall.deleted_at IS NULL
          ORDER BY rueckfall.created_at, rueckfall.id
          LIMIT 1
        )
      )
      AND note.kind = 'note'
      AND note.deleted_at IS NULL
    WHERE transcript.deleted_at IS NULL
      AND transcript.session_id <> ''
      AND transcript.id = (
        SELECT newest.id
        FROM transcripts AS newest
        WHERE newest.session_id = transcript.session_id
          AND newest.deleted_at IS NULL
        ORDER BY newest.started_at_ms DESC, newest.id DESC
        LIMIT 1
      )
`;

export async function loadScanSessions() {
  const rows = await liveQueryClient.execute<TranscriptRow>(SCAN_SESSIONS_SQL);

  return rows.map((row) => {
    const words = safeParse(row.words_json);
    const kalender = eventParticipantFields(row.event_participants_json);
    return {
      sessionId: row.session_id,
      words: (Array.isArray(words) ? (words as StoredWord[]) : []).map(
        (word) => ({
          text: typeof word.text === "string" ? word.text : "",
          measuredConfidence: measuredConfidenceOf(word),
        }),
      ),
      contextNames: [
        ...parseStringList(row.context_names_json),
        ...kalender.names,
      ],
      contextEmails: [
        ...parseStringList(row.context_emails_json),
        ...kalender.emails,
      ],
      // Der Kontext ist die Kalenderbeschreibung samt Termintitel und die
      // Notiz, beide ROH.
      //
      // Der SITZUNGSTITEL steht hier NICHT, und diese Zusage traegt den
      // ganzen Lauf: fuer Gespraeche ohne Kalendertermin schreibt ihn ein
      // Sprachmodell aus dem Transkript. Zaehlte er als Beleg, immunisierte
      // sich eine uebernommene Verhoerung selbst -- "Serredi" war genau das,
      // die Erfindung des Modells im Titel eines Gespraechs, in dem es um
      // einen anderen Namen ging. Er reist weiter in `generatedTitle`, und
      // `known_terms` in Rust schliesst dieses Feld ausdruecklich aus.
      //
      // Der Termintitel aus `event_json` steckt bereits in `context_text` --
      // dort ist er ein Beleg, weil ihn ein Mensch in den Kalender getippt
      // hat. Ein NUR ueber `event_id` aufgeloester Termin bringt seinen Titel
      // nicht mit; dieser Randfall ist bewusst offen, denn der Apparat, der
      // ihn schliessen sollte, kostete mehr als er trug.
      contextText: [row.context_text ?? "", row.note_body ?? ""]
        .filter((teil) => teil.length > 0)
        .join(" "),
      generatedTitle: row.generated_title ?? "",
    };
  });
}

/**
 * Ein Ausfall darf nie wie ein sauberes Ergebnis aussehen.
 *
 * Bis zum 03.09.2026 wurde ein Fehler aus der Bruecke zu `[]`, und die
 * Oberflaeche schrieb "nichts gefunden" -- ein Werkzeug, das schweigt, wenn es
 * kaputt ist, ist schlimmer als eines, das gar nicht laeuft. Leer und
 * unlesbar muessen unterscheidbar sein, also wirft der Fehlerfall.
 */
export async function fetchProposals({
  terms,
  dismissed,
}: {
  terms: string[];
  dismissed: string[];
}): Promise<VocabularyProposal[]> {
  const sessions = await loadScanSessions();
  if (sessions.length === 0) {
    return [];
  }
  const result = await localSttCommands.vocabularyProposals(
    sessions,
    terms,
    parseDismissed(dismissed),
  );
  if (result.status !== "ok") {
    throw new Error(String(result.error));
  }
  return result.data;
}

export const proposalsQueryKey = (terms: string[], dismissed: string[]) => [
  "vocabulary-proposals",
  terms,
  dismissed,
];

/**
 * Der Lauf startet NICHT von selbst.
 *
 * `enabled` haengt am Knopf: ueber neun Gespraeche liest der Lauf 28.458
 * Woerter und schickt sie durch die Bruecke. Das ist billig genug fuer einen
 * Knopfdruck und zu teuer, um es bei jedem Oeffnen der Einstellungen
 * ungefragt zu tun. Der selbsttaetige Lauf gehoert an eine Stelle, die den
 * Stapel ohnehin anfasst -- nicht in eine Einstellungsseite.
 *
 * `staleTime: Infinity` ist Absicht und deshalb NICHT der Weg fuer einen
 * zweiten Klick: der Aufrufer ruft `refetch()`. Ohne das waere der zweite
 * Klick ein Knopf, der nichts tut und es auch nicht sagt -- neue Transkripte
 * blieben ungesehen.
 */
export function useProposals({
  terms,
  dismissed,
  enabled,
}: {
  terms: string[];
  dismissed: string[];
  enabled: boolean;
}) {
  return useQuery({
    queryKey: proposalsQueryKey(terms, dismissed),
    queryFn: () => fetchProposals({ terms, dismissed }),
    enabled,
    staleTime: Infinity,
    // Ein Fehler ist ein Befund, kein Grund es dreimal zu versuchen.
    retry: false,
  });
}
