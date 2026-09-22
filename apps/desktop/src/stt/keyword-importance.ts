// Ersatz fuer den Wortarten-Tagger `retext-pos` (npm `pos`, LGPL-3.0).
//
// `retext-pos` ist ein englischer Penn-Treebank-Tagger mit einem fest
// einprogrammierten Lexikon englischer Wortformen (`pos/lexicon.js`). Fuer
// jedes Wort, das der Tagger nicht kennt, faellt er auf den Standardwert
// "NN" (Substantiv) zurueck (`pos/POSTagger.js:46`).
//
// Gemessen an echten deutschen Notizen (22.09.2026, Skript nicht
// eingecheckt): weil praktisch jedes deutsche Wort fuer diesen Tagger
// unbekannt ist, faellt es auf "NN" zurueck und gilt fuer
// `retext-keywords` damit als "wichtig" -- der Tagger filtert fuer
// deutschen Text also faktisch NICHTS. Er liess sogar deutsche Fuellwoerter
// wie "mit", "hat", "war" und "fuer" als Stichwort durch, waehrend er
// vereinzelte echte Inhaltswoerter verwarf, nur weil sie zufaellig wie ein
// englisches Funktionswort aussehen ("die" -> engl. Verb "to die" -> nicht
// "N..."-getaggt). Eine Stoppwortliste leistet fuer diesen Zweck mindestens
// dasselbe wie der Tagger, ohne den Zufall und ohne die LGPL-Bedingung.
//
// Die eigentliche Auswahl-Logik (Haeufigkeit, Wortstamm-Gruppierung,
// Phrasenbildung) bleibt bei `retext-keywords` (MIT) -- deren `important()`
// fragt nur `node.data.partOfSpeech` ab (Praefix "N", oder exakt "JJ" bei
// Grossschreibung). Dieses Modul ersetzt nur den Tagging-Schritt davor.

import type { Plugin } from "unified";

import { isStopword } from "./stopwords";

/** Tag, den `retext-keywords` als Substantiv/wichtig einstuft (Praefix "N"). */
const IMPORTANT_TAG = "NN";
/** Irgendein Tag ohne Praefix "N" und ungleich "JJ" -- zaehlt als unwichtig. */
const UNIMPORTANT_TAG = "XX";

const MIN_WORD_LENGTH = 2;

/**
 * Ob ein Wort ein Stichwort-Kandidat ist.
 *
 * Grundregel: kein Fuellwort (deutsche + englische Stoppwortliste) und
 * mindestens zwei Zeichen mit wenigstens einem Buchstaben.
 *
 * Grossschreibung mitten im Satz gilt zusaetzlich als Eigennamen-Signal fuer
 * Deutsch: im Deutschen sind alle Substantive grossgeschrieben, ein
 * grossgeschriebenes Wort ist also auch dann ein Kandidat, wenn dieselbe
 * Zeichenkette klein geschrieben ein Fuellwort waere (Nachname "Wird" gegen
 * das Hilfsverb "wird"). Grossschreibung nur am Satzanfang zaehlt nicht --
 * das ist Orthographie, kein Signal.
 */
export function isImportantWord(
  word: string,
  options: { sentenceInitial: boolean },
): boolean {
  const trimmed = word.trim();
  if (trimmed.length < MIN_WORD_LENGTH || !/\p{L}/u.test(trimmed)) {
    return false;
  }

  if (!isStopword(trimmed)) {
    return true;
  }

  if (options.sentenceInitial) {
    return false;
  }

  const first = trimmed.charAt(0);
  return first === first.toUpperCase() && first !== first.toLowerCase();
}

/**
 * Schlanker Baumausschnitt fuer die Traversierung: alles, was wir aus einem
 * nlcst-Knoten brauchen, um Saetze zu finden, deren Wortknoten zu lesen und
 * `data.partOfSpeech` zu setzen. Vermeidet die `@types/nlcst`-Typen, die
 * ausserhalb von retext-Paketen selbst nicht aufloesbar sind (nicht an die
 * Wurzel von `node_modules` gehoben, siehe pnpm-Baum).
 */
type WalkNode = {
  readonly type: string;
  readonly children?: readonly WalkNode[];
  readonly value?: string;
  data?: Record<string, unknown>;
};

function nodeText(node: WalkNode): string {
  if (typeof node.value === "string") {
    return node.value;
  }
  return (node.children ?? []).map(nodeText).join("");
}

function tagSentenceWords(sentence: WalkNode): void {
  let sentenceInitial = true;

  for (const child of sentence.children ?? []) {
    if (child.type !== "WordNode") continue;

    const value = nodeText(child);
    const data = child.data ?? (child.data = {});
    data.partOfSpeech = isImportantWord(value, { sentenceInitial })
      ? IMPORTANT_TAG
      : UNIMPORTANT_TAG;
    sentenceInitial = false;
  }
}

/**
 * Findet alle Satzknoten im Baum (unabhaengig von der Tiefe -- ein Satz kann
 * in einem Absatz stecken oder theoretisch direkt unter der Wurzel) und
 * taggt ihre Wortknoten. Entspricht `visit(tree, 'SentenceNode', fn)` mit
 * SKIP aus `retext-pos`, nur ohne die Abhaengigkeit `unist-util-visit`.
 */
function walk(node: WalkNode): void {
  if (node.type === "SentenceNode") {
    tagSentenceWords(node);
    return;
  }
  for (const child of node.children ?? []) {
    walk(child);
  }
}

// `Plugin<[]>` (aus `unified`, bereits direkte Abhaengigkeit) statt des
// nlcst-`Root`-Typs: `@types/nlcst` ist ausserhalb der retext-Pakete selbst
// nicht aufloesbar (nicht an die Wurzel von `node_modules` gehoben, siehe
// pnpm-Baum), waehrend `unified` seinen eigenen `Plugin`-Typ am Paket-Wurzel
// exportiert -- damit bleibt `.use(tagWordImportance)` in der Kette mit
// `retextEnglish`/`retextKeywords` typkompatibel (ein reiner `unknown`-Param
// war es nicht: `unified`s `.use()`-Ueberladungen erwarten einen konkreten
// `Node`-Typ, sonst bricht die Typinferenz fuer die folgenden `.use()`-Aufrufe).
const tagWordImportance: Plugin<[]> = () => {
  return function (tree): undefined {
    // `tree` ist zur Laufzeit ein nlcst-Knoten (Root), typisiert aber nur als
    // der generische `unist`-`Node`. Ein einziger, begruendeter Cast hier
    // statt Typreibung in der ganzen Traversierung.
    walk(tree as unknown as WalkNode);
    return undefined;
  };
};

export default tagWordImportance;
