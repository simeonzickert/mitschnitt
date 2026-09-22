/**
 * Die eine Stelle in TypeScript, die das Format einer Woerterbuch-Zeile kennt.
 *
 * Der Vertrag gehoert NICHT uns: gelesen wird die Zeile vom Rust-Crate
 * `vocabulary` (`Vocabulary::parse`, `crates/vocabulary/src/lib.rs`). Alles
 * hier ist die Schreibseite desselben Formats -- eine Zeile, die wir schreiben
 * und die Rust anders liest, ist ein Alias, der stumm nicht wirkt.
 *
 * ```text
 * Sedacz => Sedatsch; Sedatz; Seda das
 * Webflow
 * ```
 *
 * Links die Schreibweise, die im Transkript stehen soll; rechts die
 * Verhoerungen, die auf sie zurueckgeschrieben werden. Eine Zeile ohne Pfeil
 * ist ein Eintrag ohne bekannte Verhoerungen.
 *
 * Ein Rust-Test in `crates/vocabulary` liest eine hier erzeugte Zeile gegen und
 * bricht, wenn die beiden Seiten auseinanderlaufen.
 */

import { normalizeKeywordList } from "./keywords";

/** Trennt die richtige Schreibweise von ihren Verhoerungen. */
export const ALIAS_ARROW = "=>";
/** Trennt die Verhoerungen untereinander. */
export const ALIAS_SEPARATOR = ";";

/**
 * Groesste Wortzahl, die eine Verhoerung haben darf.
 *
 * Nicht gewaehlt, sondern uebernommen: der Nachlauf schiebt hoechstens
 * `Options::max_phrase_tokens` Woerter zusammen, und der steht in
 * `crates/vocabulary/src/matcher.rs` auf 3 (die laengste eingetragene Verhoerung ist
 * "NOR Druck Technik"). Ein laengerer Eintrag koennte gar nicht mehr treffen --
 * er waere ein Eintrag, der aussieht, als wuerde er wirken.
 */
export const MAX_ALIAS_WORDS = 3;

export type DictionaryEntry = {
  canonical: string;
  aliases: string[];
};

/**
 * Der Vergleichsschluessel -- bewusst `toLowerCase`, nicht `toLocaleLowerCase`.
 *
 * Rusts `to_lowercase` ist locale-unabhaengig. Im tuerkischen Locale macht
 * JavaScript aus "I" ein "\u0131" statt "i"; jeder Eintrag mit grossem I wuerde
 * dann auf beiden Seiten anders verglichen, und der Alias hoerte fuer diesen
 * Nutzer stumm auf zu wirken.
 */
export const dictionaryEntryKey = (value: string): string =>
  value.trim().replace(/\s+/g, " ").toLowerCase();

const key = dictionaryEntryKey;

/**
 * Nimmt einem Textstueck jedes Zeichen, das die Zeile zerschneiden wuerde.
 *
 * Ohne das koennte ein Mensch "A; B" als Namen eintragen und der Rust-Parser
 * saehe danach zwei Aliasse -- ein Eintrag, der etwas anderes bedeutet als das,
 * was im Feld stand.
 */
const sanitize = (value: string): string =>
  value
    .replace(/=>/g, " ")
    .split(ALIAS_SEPARATOR)
    .join(" ")
    .replace(/[\r\n]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");

/**
 * Dasselbe Abschneiden, das `core_of` in `crates/vocabulary/src/matcher.rs`
 * VOR dem Nachschlagen macht: an beiden Enden jedes Wortes faellt weg, was
 * weder Buchstabe noch Ziffer noch `-` oder `'` ist.
 *
 * Ohne das speichert eine Korrektur an "Sarec," die Verhoerung MIT Komma,
 * waehrend der Nachlauf "sarec" sucht -- ein Alias, der gespeichert ist,
 * gemeldet wird und nie trifft. Bindestrich und Apostroph bleiben, weil Rust
 * sie ausdruecklich stehen laesst ("Tofmann-Gruppe", "O'Brien").
 */
const ALIAS_EDGE = /^[^\p{L}\p{N}\-']+|[^\p{L}\p{N}\-']+$/gu;

const sanitizeAlias = (value: string): string =>
  sanitize(value)
    .split(" ")
    .map((word) => word.replace(ALIAS_EDGE, ""))
    .filter((word) => word.length > 0)
    .join(" ");

/**
 * Liest eine gespeicherte Zeile so, wie `Vocabulary::parse` sie liest.
 *
 * Gibt `null` zurueck, wo Rust die Zeile fallen laesst: leer, oder ohne
 * kanonischen Teil links vom Pfeil.
 */
export function parseDictionaryEntry(line: string): DictionaryEntry | null {
  const trimmed = line.trim();
  if (trimmed.length === 0) {
    return null;
  }

  // `split_once` in Rust: die ERSTE Fundstelle trennt.
  const arrowAt = trimmed.indexOf(ALIAS_ARROW);
  const canonical = (
    arrowAt === -1 ? trimmed : trimmed.slice(0, arrowAt)
  ).trim();
  if (canonical.length === 0) {
    return null;
  }

  const aliasPart =
    arrowAt === -1 ? "" : trimmed.slice(arrowAt + ALIAS_ARROW.length);
  const canonicalKey = key(canonical);
  const seen = new Set<string>();
  const aliases: string[] = [];

  for (const raw of aliasPart.split(ALIAS_SEPARATOR)) {
    const alias = raw.trim().replace(/\s+/g, " ");
    if (alias.length === 0) {
      continue;
    }
    const aliasKey = key(alias);
    // Ein Alias, der dem Namen gleicht, waere eine Ersetzung durch sich selbst.
    if (aliasKey === canonicalKey || seen.has(aliasKey)) {
      continue;
    }
    seen.add(aliasKey);
    aliases.push(alias);
  }

  return { canonical, aliases };
}

/** Schreibt einen Eintrag als genau die Zeile, die `parseDictionaryEntry` und
 * `Vocabulary::parse` wieder auseinandernehmen. */
export function formatDictionaryEntry(entry: DictionaryEntry): string {
  const canonical = sanitize(entry.canonical);
  const canonicalKey = key(canonical);
  const seen = new Set<string>();
  const aliases: string[] = [];

  for (const raw of entry.aliases) {
    const alias = sanitizeAlias(raw);
    const aliasKey = key(alias);
    if (alias.length === 0 || aliasKey === canonicalKey || seen.has(aliasKey)) {
      continue;
    }
    seen.add(aliasKey);
    aliases.push(alias);
  }

  return aliases.length === 0
    ? canonical
    : `${canonical} ${ALIAS_ARROW} ${aliases.join(`${ALIAS_SEPARATOR} `)}`;
}

/**
 * Alle Eintraege einer gespeicherten Liste -- und zwar zusammengelegt.
 *
 * Zweimal derselbe Name in der Liste ist EIN Eintrag mit beiden Verhoerungen.
 * Das ist keine Bequemlichkeit, sondern die Regel aus `Vocabulary::parse`
 * (`crates/vocabulary/src/lib.rs`): wer hier zwei Eintraege zurueckgaebe,
 * zeigte dem Menschen zwei Zeilen, wo der Nachlauf eine sieht -- und beim
 * Zurueckschreiben verloere eine davon ihre Verhoerungen.
 */
export function parseDictionaryEntries(terms: string[]): DictionaryEntry[] {
  const entries: DictionaryEntry[] = [];
  const positionOf = new Map<string, number>();

  for (const term of terms) {
    const entry = parseDictionaryEntry(term);
    if (!entry) {
      continue;
    }
    const entryKey = key(entry.canonical);
    const at = positionOf.get(entryKey);
    if (at === undefined) {
      positionOf.set(entryKey, entries.length);
      entries.push(entry);
      continue;
    }
    for (const alias of entry.aliases) {
      if (!entries[at].aliases.some((known) => key(known) === key(alias))) {
        entries[at].aliases.push(alias);
      }
    }
  }

  return entries;
}

/**
 * Aus einer Woerterbuch-Zeile nur die richtige Schreibweise.
 *
 * Das ist der HINWEIS an ein Modell und an die Anbieter. Die Verhoerungen
 * duerfen dort nicht hin -- einem Modell "Sarec" als erwuenschtes Wort
 * vorzulegen waere das Gegenteil der Absicht.
 */
export function dictionaryCanonicalTerms(terms: string[]): string[] {
  return normalizeKeywordList(
    parseDictionaryEntries(terms).map((entry) => entry.canonical),
  );
}

/**
 * Haengt eine Verhoerung an den Eintrag mit dieser Schreibweise -- und legt
 * keinen zweiten Eintrag desselben Namens an.
 *
 * Gibt die Liste unveraendert zurueck, wenn die Verhoerung schon dort steht.
 */
export function addDictionaryAlias(
  terms: string[],
  canonical: string,
  alias: string,
): string[] {
  const canonicalKey = key(canonical);
  const aliasKey = key(alias);
  if (canonicalKey.length === 0 || aliasKey.length === 0) {
    return terms;
  }

  // Erst schauen, DANN schreiben. `Vocabulary::parse` legt alle Zeilen mit
  // demselben Namen zu EINEM Eintrag zusammen -- steht die Verhoerung also in
  // der zweiten Zeile, kennt der Nachlauf sie laengst. Wer nur bis zur ersten
  // Zeile schaut, haengt sie ein zweites Mal an und meldet sie als frisch
  // gelernt, obwohl sich fuer den Nachlauf nichts geaendert hat.
  const entries = terms.map((term) => parseDictionaryEntry(term));
  const matching = entries.filter(
    (entry) => entry !== null && key(entry.canonical) === canonicalKey,
  );
  const found = matching.length > 0;
  const alreadyKnown = matching.some((entry) =>
    entry!.aliases.some((known) => key(known) === aliasKey),
  );

  let changed = false;
  const next = terms.map((term, at) => {
    const entry = entries[at];
    if (!entry || key(entry.canonical) !== canonicalKey || alreadyKnown) {
      return term;
    }
    if (changed) {
      return term;
    }
    changed = true;
    return formatDictionaryEntry({
      canonical: entry.canonical,
      aliases: [...entry.aliases, alias],
    });
  });

  if (found) {
    // Bei einem Nicht-Ereignis kommt die EINGABE zurueck, nicht eine gleiche
    // Kopie. Der Aufrufer prueft mit `!==`, ob etwas passiert ist -- gaebe es
    // hier immer ein neues Array, waere seine Pruefung immer wahr und er
    // meldete bei jeder Wiederholung derselben Korrektur einen frisch
    // gelernten Alias, obwohl die Liste unveraendert bleibt.
    return changed ? next : terms;
  }

  return [...next, formatDictionaryEntry({ canonical, aliases: [alias] })];
}

/**
 * Ersetzt GENAU den Eintrag mit diesem Vergleichsschluessel durch den neuen --
 * gerechnet gegen die Liste, die in diesem Moment WIRKLICH gespeichert ist,
 * nicht gegen eine Anzeige, die seit dem Oeffnen des Bearbeitungsfelds
 * veraltet sein kann (derselbe Grundsatz wie bei `onSave` in
 * `settings/dictionary/index.tsx`: rechnen beim SCHREIBEN, nicht beim Klick).
 *
 * Arbeitet auf den ZUSAMMENGELEGTEN Eintraegen (`parseDictionaryEntries`),
 * nicht auf den rohen Zeilen: stuende derselbe kanonische Name zufaellig auf
 * zwei Zeilen, wuerde ein Ersetzen nur der ERSTEN die zweite unveraendert
 * unter dem ALTEN Namen stehen lassen -- der Eintrag waere nicht bearbeitet,
 * sondern verdoppelt. `removeTerm` normalisiert deshalb schon vor jedem
 * Schreiben genauso (`normalisiere`); dieselbe Konsolidierung ist hier also
 * kein Nebeneffekt, sondern die Fortsetzung der bestehenden Regel.
 *
 * Gibt die Liste UNVERAENDERT zurueck, wenn
 * - der alte Name laengst weg ist (zwei Fenster, zwei Loeschungen), oder
 * - der neue Name mit einem ANDEREN vorhandenen Eintrag kollidiert.
 *   `Vocabulary::parse` wuerde die beiden sonst zu einem zusammenlegen und
 *   dabei eine der beiden Verhoerungslisten stillschweigend verschmelzen --
 *   das ist kein "bearbeitet", das ist Datenverlust.
 */
export function replaceDictionaryEntry(
  terms: string[],
  originalCanonicalKey: string,
  nextEntry: DictionaryEntry,
): string[] {
  const entries = parseDictionaryEntries(terms);
  const at = entries.findIndex(
    (entry) => key(entry.canonical) === originalCanonicalKey,
  );
  if (at === -1) {
    return terms;
  }

  const nextKey = key(nextEntry.canonical);
  const collides = entries.some(
    (entry, index) => index !== at && key(entry.canonical) === nextKey,
  );
  if (collides) {
    return terms;
  }

  return entries.map((entry, index) =>
    formatDictionaryEntry(index === at ? nextEntry : entry),
  );
}

/**
 * Darf aus diesem Korrektur-Paar ueberhaupt eine Verhoerung werden?
 *
 * Der harte Waechter -- ob die Verhoerung selbst gewoehnliches Deutsch ist --
 * sitzt in Rust (`stopwords::is_all_common_german_phrase`) und wird hier NICHT
 * nachgebaut. Hier steht nur die Formfrage: eine Verhoerung ist ein Wort oder
 * eine kurze Wortfolge, keine Satzkorrektur. Ohne diese Schranke wuerde aus
 * "wir haben das besprochen" -> "Sedacz" ein Alias, der eine ganze Aussage
 * ueberschreibt.
 */
export function isAliasCandidate(alias: string, canonical: string): boolean {
  const aliasText = sanitizeAlias(alias);
  const canonicalText = sanitize(canonical);
  if (aliasText.length === 0 || canonicalText.length === 0) {
    return false;
  }
  if (key(aliasText) === key(canonicalText)) {
    return false;
  }
  return (
    aliasText.split(" ").length <= MAX_ALIAS_WORDS &&
    canonicalText.split(" ").length <= MAX_ALIAS_WORDS
  );
}

/**
 * Was ein Mensch ins Verhoerungs-Feld tippt.
 *
 * Komma UND Semikolon gelten als Trenner: gespeichert wird mit Semikolon (das
 * liest Rust), getippt wird meistens mit Komma.
 */
export function parseAliasInput(value: string): string[] {
  return value
    .split(/[;,\n]/)
    .map((alias) => sanitizeAlias(alias))
    .filter((alias) => alias.length > 0);
}
