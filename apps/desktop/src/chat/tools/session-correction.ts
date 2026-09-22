import { tool } from "ai";
import { z } from "zod";

import { md2json } from "@anlg/editor/markdown";

import type { ToolDependencies } from "./types";

import {
  applySessionContentCorrections,
  type SummaryContentCorrection,
  type TranscriptContentCorrection,
} from "~/session/content-mutations";
import {
  loadSessionContentSnapshot,
  type SessionContentSnapshot,
} from "~/session/content-queries";
import { updateSettingValue } from "~/settings/queries";
import {
  addDictionaryAlias,
  dictionaryEntryKey,
  isAliasCandidate,
  parseDictionaryEntry,
} from "~/stt/dictionary-entry";
import { normalizeKeywordList, parseDictionaryTermsJson } from "~/stt/keywords";
import { fetchRiskyAliasLookup } from "~/stt/risky-aliases";
import {
  createTranscriptTimingMetadata,
  mergeMeasuredConfidence,
  withWordConfidence,
  withoutWordConfidence,
} from "~/stt/timing";

type CorrectionTarget = "summary" | "transcript" | "summary_and_transcript";

type ReplacementResult = {
  text: string;
  count: number;
};

type TranscriptWord = {
  id?: string | null;
  text?: string | null;
  start_ms?: number | null;
  end_ms?: number | null;
  channel?: number | null;
  speaker?: string | null;
  metadata?: unknown;
};

type SummaryChange = {
  enhancedNoteId: string;
  title: string;
  replacements: number;
};

type TranscriptChange = {
  transcriptId: string;
  wordReplacements: number;
  memoReplacements: number;
};

type LearnedAlias = {
  canonical: string;
  alias: string;
};

type DictionaryChange = {
  addedTerms: string[];
  /** Verhoerungen, die diese Korrektur der Liste beigebracht hat. */
  learnedAliases: LearnedAlias[];
  /**
   * Verhoerungen, die NICHT gelernt wurden, obwohl die Form gestimmt haette --
   * weil der Nachlauf sie ohnehin verwerfen wuerde.
   *
   * Der Unterschied ist kein Etikett: was hier steht, darf das Modell dem
   * Menschen nicht als gemerkt melden.
   */
  ignoredAliases: LearnedAlias[];
};

type TitleChange = {
  replacements: number;
  nextTitle: string;
};

type SummaryCorrectionPlan = {
  changes: SummaryChange[];
  updates: SummaryContentCorrection[];
};

type TranscriptCorrectionPlan = {
  changes: TranscriptChange[];
  updates: TranscriptContentCorrection[];
};

function replaceExact(
  value: string,
  oldText: string,
  newText: string,
): ReplacementResult {
  if (!oldText) {
    return { text: value, count: 0 };
  }

  const parts = value.split(oldText);
  if (parts.length === 1) {
    return { text: value, count: 0 };
  }

  return {
    text: parts.join(newText),
    count: parts.length - 1,
  };
}

function replaceBoundedExact(
  value: string,
  oldText: string,
  newText: string,
): ReplacementResult {
  if (!oldText) {
    return { text: value, count: 0 };
  }

  const escaped = oldText.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(
    `(^|[^\\p{L}\\p{N}])${escaped}(?=$|[^\\p{L}\\p{N}])`,
    "giu",
  );
  let count = 0;
  const text = value.replace(pattern, (_match, prefix: string) => {
    count++;
    return `${prefix}${newText}`;
  });

  return { text, count };
}

function planSummaryCorrections({
  notes,
  enhancedNoteId,
  oldText,
  newText,
}: {
  notes: SessionContentSnapshot["enhancedNotes"];
  enhancedNoteId?: string;
  oldText: string;
  newText: string;
}): SummaryCorrectionPlan {
  const candidates = enhancedNoteId
    ? notes.filter((note) => note.id === enhancedNoteId)
    : notes;
  const changes: SummaryChange[] = [];
  const updates: SummaryContentCorrection[] = [];

  for (const note of candidates) {
    const replaced = replaceExact(note.markdown, oldText, newText);
    if (replaced.count === 0) {
      continue;
    }

    updates.push({
      id: note.id,
      currentContent: note.content,
      currentContentFormat: note.contentFormat,
      nextContent: JSON.stringify(md2json(replaced.text)),
    });
    changes.push({
      enhancedNoteId: note.id,
      title: note.title.trim() || "Summary",
      replacements: replaced.count,
    });
  }

  return { changes, updates };
}

function trimTokenPunctuation(value: string): string {
  return value.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, "") || value;
}

function normalizeComparableToken(value: string): string {
  return value
    .normalize("NFKC")
    .toLocaleLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, "");
}

function tokenizeReplacement(value: string): string[] {
  return value
    .split(/\s+/)
    .map((item) => item.trim())
    .map(trimTokenPunctuation)
    .filter(Boolean);
}

function tokenizeComparable(value: string): string[] {
  return tokenizeReplacement(value)
    .map(normalizeComparableToken)
    .filter(Boolean);
}

function wordRangeMatchesAt(
  words: TranscriptWord[],
  target: string[],
  start: number,
): boolean {
  if (target.length === 0 || start + target.length > words.length) {
    return false;
  }

  return target.every(
    (text, index) =>
      normalizeComparableToken(words[start + index].text ?? "") === text,
  );
}

function buildReplacementWords(
  original: TranscriptWord[],
  newText: string,
): TranscriptWord[] {
  const tokens = tokenizeReplacement(newText);
  if (tokens.length === 0) {
    return [];
  }

  const first = original[0] ?? {};
  const last = original[original.length - 1] ?? first;

  // Wort fuer Wort ersetzt (der haeufige Fall: ein Name anders geschrieben).
  // Dann behaelt jedes Wort SEINE Zeit. Ohne diesen Zweig zerstoerte jede
  // Korrektur die gemessenen Wortzeiten des Erkenners und verteilte sie wieder
  // gleichmaessig -- an einer Stelle, an der gar nichts zu verteilen ist.
  if (tokens.length === original.length) {
    return tokens.map((text, index) => ({ ...original[index], text }));
  }

  const startMs = typeof first.start_ms === "number" ? first.start_ms : null;
  const endMs = typeof last.end_ms === "number" ? last.end_ms : startMs;
  const duration =
    startMs !== null && endMs !== null ? Math.max(endMs - startMs, 0) : 0;
  const step = tokens.length > 0 ? duration / tokens.length : 0;
  const baseId = typeof first.id === "string" && first.id ? first.id : null;
  // Fork (03.09.2026): dieselbe vorsichtige Rechnung wie der Rust-Nachlauf in
  // `crates/listener2-core/src/vocabulary.rs` (`split_replacement`). Ohne sie
  // erbte jedes neue Wort die gemessene Sicherheit des ERSTEN -- aus 0,91 und
  // 0,42 wurden dreimal 0,91, und eine erfundene Zahl stand dauerhaft in
  // words_json.
  const mergedConfidence = mergeMeasuredConfidence(
    original.map((word) => word.metadata),
  );

  return tokens.map((text, index) => {
    const wordStart =
      startMs === null ? first.start_ms : Math.round(startMs + step * index);
    const wordEnd =
      startMs === null
        ? first.end_ms
        : Math.round(startMs + step * (index + 1));

    return {
      ...first,
      id: index === 0 ? first.id : `${baseId ?? "word"}:correction:${index}`,
      text,
      start_ms: wordStart,
      end_ms: wordEnd,
      // Andere Wortzahl heisst: die Zeiten sind ab hier verteilt, nicht
      // gemessen. Das Etikett des ersten Wortes weiterzuerben hiesse, eine
      // Schaetzung als Messung auszugeben.
      metadata: withWordConfidence(
        withoutWordConfidence(
          createTranscriptTimingMetadata(
            "provider_segment_interpolated",
            first.metadata,
          ),
        ),
        mergedConfidence,
      ),
    };
  });
}

function replaceTranscriptWords(
  words: TranscriptWord[],
  oldText: string,
  newText: string,
): { words: TranscriptWord[]; count: number } {
  const target = tokenizeComparable(oldText);
  const replacementTokens = tokenizeReplacement(newText);
  if (
    target.length === 0 ||
    replacementTokens.length === 0 ||
    target.length > words.length
  ) {
    return { words, count: 0 };
  }

  const nextWords: TranscriptWord[] = [];
  let count = 0;
  for (let index = 0; index < words.length; ) {
    if (target.length === 1 && replacementTokens.length === 1) {
      const word = words[index];
      const replaced = replaceBoundedExact(word.text ?? "", oldText, newText);
      if (replaced.count > 0) {
        nextWords.push({ ...word, text: replaced.text });
        count += replaced.count;
        index++;
        continue;
      }
    }

    if (wordRangeMatchesAt(words, target, index)) {
      const original = words.slice(index, index + target.length);
      nextWords.push(...buildReplacementWords(original, newText));
      index += target.length;
      count++;
      continue;
    }

    nextWords.push(words[index]);
    index++;
  }

  return count === 0 ? { words, count: 0 } : { words: nextWords, count };
}

const TEXT_TOKEN_PATTERN = /[\p{L}\p{N}]+(?:['’_-][\p{L}\p{N}]+)*/gu;

function findComparableTextTokens(
  value: string,
): Array<{ text: string; start: number; end: number }> {
  return Array.from(value.matchAll(TEXT_TOKEN_PATTERN)).flatMap((match) => {
    const start = match.index;
    if (start === undefined) {
      return [];
    }

    const text = normalizeComparableToken(match[0]);
    return text ? [{ text, start, end: start + match[0].length }] : [];
  });
}

function replaceLoosePhrase(
  value: string,
  oldText: string,
  newText: string,
): ReplacementResult {
  const target = tokenizeComparable(oldText);
  const tokens = findComparableTextTokens(value);
  if (target.length === 0 || target.length > tokens.length) {
    return { text: value, count: 0 };
  }

  const parts: string[] = [];
  let cursor = 0;
  let count = 0;

  for (let index = 0; index < tokens.length; ) {
    const matches = target.every(
      (text, offset) => tokens[index + offset]?.text === text,
    );
    if (!matches) {
      index++;
      continue;
    }

    const first = tokens[index];
    const last = tokens[index + target.length - 1];
    parts.push(value.slice(cursor, first.start), newText);
    cursor = last.end;
    count++;
    index += target.length;
  }

  if (count === 0) {
    return { text: value, count: 0 };
  }

  parts.push(value.slice(cursor));
  return { text: parts.join(""), count };
}

function replaceTranscriptText(
  value: string,
  oldText: string,
  newText: string,
): ReplacementResult {
  if (
    tokenizeComparable(oldText).length === 1 &&
    tokenizeReplacement(newText).length === 1
  ) {
    const bounded = replaceBoundedExact(value, oldText, newText);
    return bounded.count > 0
      ? bounded
      : replaceLoosePhrase(value, oldText, newText);
  }

  const exact = replaceExact(value, oldText, newText);
  return exact.count > 0 ? exact : replaceLoosePhrase(value, oldText, newText);
}

function planTranscriptCorrections({
  transcripts,
  oldText,
  newText,
}: {
  transcripts: SessionContentSnapshot["transcripts"];
  oldText: string;
  newText: string;
}): TranscriptCorrectionPlan {
  if (tokenizeReplacement(newText).length === 0) {
    return { changes: [], updates: [] };
  }

  const changes: TranscriptChange[] = [];
  const updates: TranscriptContentCorrection[] = [];

  for (const transcript of transcripts) {
    const words = transcript.words as TranscriptWord[];
    const wordResult = replaceTranscriptWords(words, oldText, newText);

    const hasMemo = transcript.memo.length > 0;
    const memoResult = replaceTranscriptText(
      hasMemo ? transcript.memo : "",
      oldText,
      newText,
    );

    if (wordResult.count === 0 && memoResult.count === 0) {
      continue;
    }
    if (
      words.length > 0 &&
      hasMemo &&
      (wordResult.count === 0 || memoResult.count === 0)
    ) {
      continue;
    }

    updates.push({
      id: transcript.id,
      currentWordsJson: transcript.wordsJson,
      currentMemo: transcript.memo,
      nextWordsJson:
        wordResult.count > 0
          ? JSON.stringify(wordResult.words)
          : transcript.wordsJson,
      nextMemo: memoResult.count > 0 ? memoResult.text : transcript.memo,
    });

    changes.push({
      transcriptId: transcript.id,
      wordReplacements: wordResult.count,
      memoReplacements: memoResult.count,
    });
  }

  return { changes, updates };
}

function planTitleCorrection({
  title,
  oldText,
  newText,
}: {
  title: string;
  oldText: string;
  newText: string;
}): TitleChange | null {
  const replaced = replaceExact(title, oldText, newText);
  if (replaced.count === 0 || replaced.text === title) {
    return null;
  }

  return {
    replacements: replaced.count,
    nextTitle: replaced.text,
  };
}

/**
 * Der Schluessel eines Eintrags ist seine richtige SCHREIBWEISE, nicht die
 * ganze Zeile.
 *
 * Sonst waeren "Sedacz" und "Sedacz => Sarec" zwei verschiedene Eintraege,
 * und jede zweite Korrektur legte eine Dublette an -- von denen der Nachlauf
 * beim Nachschlagen keine mehr eindeutig aufloest.
 */
function dictionaryKey(value: string): string {
  const entry = parseDictionaryEntry(value);
  // `dictionaryEntryKey` und nicht eine eigene Zeile: die Schluesselbildung
  // muss ueberall dieselbe sein und locale-unabhaengig bleiben (Rusts
  // `to_lowercase` ist es).
  return dictionaryEntryKey(entry?.canonical ?? value);
}

/**
 * Speichert, was die Korrektur ueber die Sprache gelernt hat.
 *
 * Bis hierher wurde nur `newText` abgelegt -- die richtige Schreibweise ohne
 * die Verhoerung, auf die sie zeigt. Damit ersetzt der Nachlauf nichts: er
 * kennt keinen Alias und laesst "Sarec" stehen. Genau das stand am 02.09.2026
 * im Protokoll (`vocabulary_pass_completed entries=20 replacements=0`).
 *
 * `oldText` ist der fehlende halbe Satz. Er wird nur dort angehaengt, wo die
 * Korrektur ihn wirklich hergibt: der Begriff muss der sein, durch den ersetzt
 * wurde (`newText`), und das Paar muss die Form einer Verhoerung haben und
 * nicht die einer Satzkorrektur.
 *
 * Ob die Verhoerung selbst gewoehnliches Deutsch ist, entscheidet weiterhin
 * Rust (`stopwords::is_all_common_german_phrase`) -- der Waechter wird hier
 * nicht nachgebaut, sondern in der Oberflaeche sichtbar gemacht.
 */
async function saveDictionaryTerms(
  terms?: string[],
  correction?: { oldText: string; newText: string },
): Promise<DictionaryChange> {
  const alias = correction?.oldText.trim() ?? "";
  const replacement = correction?.newText.trim() ?? "";
  const canLearn =
    alias.length > 0 &&
    replacement.length > 0 &&
    isAliasCandidate(alias, replacement);

  if ((!terms || terms.length === 0) && !canLearn) {
    return { addedTerms: [], learnedAliases: [], ignoredAliases: [] };
  }

  let addedTerms: string[] = [];
  let learnedAliases: LearnedAlias[] = [];
  let ignoredAliases: LearnedAlias[] = [];

  // Vor dem Schreiben gefragt, weil der Updater unten synchron laeuft -- und
  // gefragt wird Rust selbst (`Vocabulary::risky_aliases`), nicht eine Kopie
  // seiner Stoppwortliste.
  const istVerworfen = canLearn
    ? await fetchRiskyAliasLookup([`${replacement} => ${alias}`])
    : () => false;

  await updateSettingValue(
    "personalization_dictionary_terms",
    (storedValue) => {
      const currentTerms = parseDictionaryTermsJson(storedValue);
      const currentKeys = new Set(currentTerms.map(dictionaryKey));
      addedTerms = normalizeKeywordList(terms ?? []).filter(
        (term) => !currentKeys.has(dictionaryKey(term)),
      );
      learnedAliases = [];
      ignoredAliases = [];

      let nextTerms = normalizeKeywordList([...currentTerms, ...addedTerms]);
      // Ohne diese Schranke wuerde jede kurze Sachkorrektur zur Dauerregel:
      // "Montag" -> "Dienstag" ist formal ein Ein-Wort-Paar und legte bei
      // leerer Liste den Eintrag "Dienstag => Montag" an -- ungefragt, und fuer
      // immer. Eine Verhoerung wird nur an einen Namen gehaengt, den es schon
      // gibt: entweder steht er in der gespeicherten Liste, oder das Modell
      // hat ihn in DIESEM Aufruf ausdruecklich als Woerterbuch-Begriff
      // mitgeschickt. Der Rueckfall auf `newText` allein ist gestrichen.
      const gewuenschtSchonEingetragen = nextTerms.some(
        (term) => dictionaryKey(term) === dictionaryKey(replacement),
      );
      // Was der Nachlauf ohnehin verwirft, wird nicht gespeichert und schon gar
      // nicht als gemerkt gemeldet -- sonst sagte das Modell "habe ich mir
      // gemerkt" ueber einen Eintrag, den `is_all_common_german_phrase`
      // (`crates/vocabulary/src/matcher.rs`) nie anwendet.
      const verhoerungIstGewoehnlichesDeutsch =
        canLearn && istVerworfen(replacement, alias);

      if (canLearn && verhoerungIstGewoehnlichesDeutsch) {
        const kanonisch =
          nextTerms.find(
            (term) => dictionaryKey(term) === dictionaryKey(replacement),
          ) ?? replacement;
        ignoredAliases.push({
          canonical: parseDictionaryEntry(kanonisch)?.canonical ?? replacement,
          alias,
        });
      }

      if (
        canLearn &&
        gewuenschtSchonEingetragen &&
        !verhoerungIstGewoehnlichesDeutsch
      ) {
        const kandidaten = [...normalizeKeywordList(terms ?? []), ...nextTerms];
        for (const kandidat of kandidaten) {
          if (dictionaryKey(kandidat) !== dictionaryKey(replacement)) {
            continue;
          }
          const canonical =
            parseDictionaryEntry(kandidat)?.canonical ?? kandidat;
          const withAlias = addDictionaryAlias(nextTerms, canonical, alias);
          // Identitaetsvergleich, und `addDictionaryAlias` gibt bei einem
          // Nicht-Ereignis wirklich die Eingabe zurueck. Sonst meldete jede
          // Wiederholung derselben Korrektur einen frisch gelernten Alias --
          // und das Modell erzaehlte dem Menschen "habe ich mir gemerkt",
          // obwohl sich nichts geaendert hat.
          if (withAlias !== nextTerms) {
            nextTerms = withAlias;
            learnedAliases.push({ canonical, alias });
          }
          break;
        }
      }

      return JSON.stringify(nextTerms);
    },
  );

  return { addedTerms, learnedAliases, ignoredAliases };
}

function shouldEditSummary(target: CorrectionTarget): boolean {
  return target === "summary" || target === "summary_and_transcript";
}

function shouldEditTranscript(target: CorrectionTarget): boolean {
  return target === "transcript" || target === "summary_and_transcript";
}

export const buildApplySessionCorrectionTool = (
  deps: Pick<ToolDependencies, "getSessionId" | "getEnhancedNoteId">,
) =>
  tool({
    description:
      "Apply a correction to a session summary, visible session title, and/or transcript. Use this when the user corrects note content, for example 'it's not X but Y'. Prefer summary_and_transcript for factual meeting corrections unless the user explicitly asks for one target only. Read the note first if you need exact summary text.",
    inputSchema: z.object({
      sessionId: z
        .string()
        .optional()
        .describe("The session ID to edit. Defaults to the current session."),
      target: z
        .enum(["summary", "transcript", "summary_and_transcript"])
        .default("summary_and_transcript")
        .describe(
          "Which session content to correct. Use summary only or transcript only when the user explicitly scopes the correction.",
        ),
      enhancedNoteId: z
        .string()
        .optional()
        .describe(
          "Optional summary ID to restrict summary edits. Defaults to summaries in the target session.",
        ),
      oldText: z
        .string()
        .min(1)
        .describe("Exact text currently present in the note or transcript."),
      newText: z.string().describe("Replacement text."),
      dictionaryTerms: z
        .array(z.string().min(1))
        .default([])
        .describe(
          "Uncommon names, company/product names, acronyms, or jargon from the correction to save for future transcription. Skip common names.",
        ),
    }),
    execute: async (params: {
      sessionId?: string;
      target?: CorrectionTarget;
      enhancedNoteId?: string;
      oldText: string;
      newText: string;
      dictionaryTerms?: string[];
    }) => {
      const sessionId = params.sessionId ?? deps.getSessionId();
      const target = params.target ?? "summary_and_transcript";

      if (!sessionId) {
        return {
          status: "error",
          message:
            "No active session selected. Provide sessionId explicitly when calling apply_session_correction.",
        };
      }

      const newText = params.newText.trim();
      if (!newText) {
        return {
          status: "error",
          message: "Replacement text cannot be blank.",
          sessionId,
        };
      }

      const enhancedNoteId =
        params.enhancedNoteId ??
        (params.sessionId ? undefined : deps.getEnhancedNoteId());
      const snapshot = await loadSessionContentSnapshot(sessionId);
      if (!snapshot) {
        return {
          status: "error",
          message: "The target session could not be loaded.",
          sessionId,
        };
      }

      let editSummary = shouldEditSummary(target);
      if (
        editSummary &&
        enhancedNoteId &&
        !snapshot.enhancedNotes.some((note) => note.id === enhancedNoteId)
      ) {
        if (target === "summary") {
          return {
            status: "error",
            message:
              "The requested summary does not belong to the target session.",
            sessionId,
          };
        }
        editSummary = false;
      }

      const summaryPlan = editSummary
        ? planSummaryCorrections({
            notes: snapshot.enhancedNotes,
            enhancedNoteId,
            oldText: params.oldText,
            newText,
          })
        : { changes: [], updates: [] };
      const editTranscript = shouldEditTranscript(target);
      const transcriptPlan = editTranscript
        ? planTranscriptCorrections({
            transcripts: snapshot.transcripts,
            oldText: params.oldText,
            newText,
          })
        : { changes: [], updates: [] };
      const summaryChanges = summaryPlan.changes;
      const transcriptChanges = transcriptPlan.changes;
      const titleChange = editSummary
        ? planTitleCorrection({
            title: snapshot.title,
            oldText: params.oldText,
            newText,
          })
        : null;

      if (
        summaryChanges.length === 0 &&
        transcriptChanges.length === 0 &&
        !titleChange
      ) {
        return {
          status: "not_found",
          message:
            "No exact match found. Read the note and call apply_session_correction with the exact current text.",
          sessionId,
        };
      }

      try {
        await applySessionContentCorrections({
          sessionId,
          summaries: summaryPlan.updates,
          transcripts: transcriptPlan.updates,
          ...(titleChange
            ? {
                title: {
                  currentTitle: snapshot.title,
                  nextTitle: titleChange.nextTitle,
                },
              }
            : {}),
        });
      } catch (error) {
        console.error("Failed to apply session correction", error);
        return {
          status: "error",
          message:
            "The note changed before the correction could be committed. Read the note and retry.",
          sessionId,
        };
      }

      let dictionaryChanges: DictionaryChange = {
        addedTerms: [],
        learnedAliases: [],
        ignoredAliases: [],
      };
      let dictionarySaveFailed = false;
      try {
        dictionaryChanges = await saveDictionaryTerms(params.dictionaryTerms, {
          oldText: params.oldText,
          newText,
        });
      } catch (error) {
        dictionarySaveFailed = true;
        console.error("Failed to save correction dictionary terms", error);
      }
      const missingTargets = [
        editSummary && summaryChanges.length === 0 && !titleChange
          ? "summary"
          : null,
        editTranscript && transcriptChanges.length === 0 ? "transcript" : null,
      ].filter(Boolean);

      const messages = [
        missingTargets.length > 0
          ? `Applied correction where matched, but no matching ${missingTargets.join(" or ")} text was found.`
          : null,
        dictionarySaveFailed
          ? "The correction was applied, but dictionary terms could not be saved."
          : null,
      ].filter((message): message is string => Boolean(message));

      return {
        status: missingTargets.length > 0 ? "partial" : "applied",
        message: messages.length > 0 ? messages.join(" ") : undefined,
        sessionId,
        summaryChanges,
        transcriptChanges,
        titleChange,
        dictionaryChanges,
      };
    },
  });

export const sessionCorrectionTestInternals = {
  planSummaryCorrections,
  planTranscriptCorrections,
  planTitleCorrection,
  replaceExact,
  replaceLoosePhrase,
  replaceTranscriptWords,
  saveDictionaryTerms,
};
