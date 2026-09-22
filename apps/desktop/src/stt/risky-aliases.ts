/**
 * Welche eingetragenen Verhoerungen der Nachlauf verwirft -- gefragt wird der,
 * der es spaeter entscheidet.
 *
 * Bis zum 03.09.2026 lag daneben eine 962-zeilige TypeScript-Kopie der
 * deutschen Stoppwortliste (`dictionary-stopwords.ts`). Ein Test verglich sie
 * gegen die Rust-Quelle -- aber nur die WORTMENGE. Die beiden Normalisierer
 * (`stopwords::normalize` in Rust, eine handgeschriebene Schleife hier) waren
 * unabhaengiger Code: haette Rust seine Normalisierung geaendert, waere der
 * Test gruen geblieben und die gelbe Warnung in den Einstellungen still zur
 * Luege geworden.
 *
 * Jetzt gibt es nur noch eine Entscheidung, und sie steht in Rust.
 */

import { useQuery } from "@tanstack/react-query";

import { commands as localSttCommands } from "@anlg/plugin-local-stt";

import { dictionaryEntryKey } from "./dictionary-entry";

export type RiskyAliasLookup = (canonical: string, alias: string) => boolean;

/** Sagt zu nichts "verworfen" -- der Zustand, solange Rust nicht geantwortet hat. */
const NICHTS_VERWORFEN: RiskyAliasLookup = () => false;

const schluessel = (canonical: string, alias: string): string =>
  `${dictionaryEntryKey(canonical)}\u0000${dictionaryEntryKey(alias)}`;

function buildLookup(
  risky: { canonical: string; alias: string }[],
): RiskyAliasLookup {
  const set = new Set(
    risky.map((entry) => schluessel(entry.canonical, entry.alias)),
  );
  return (canonical, alias) => set.has(schluessel(canonical, alias));
}

export async function fetchRiskyAliasLookup(
  terms: string[],
): Promise<RiskyAliasLookup> {
  if (terms.length === 0) {
    return NICHTS_VERWORFEN;
  }
  const result = await localSttCommands.vocabularyRiskyAliases(terms);
  // Ein Fehler darf keine Warnung erfinden: lieber keine Warnung als eine
  // falsche.
  return result.status === "ok" ? buildLookup(result.data) : NICHTS_VERWORFEN;
}

export function useRiskyAliases(terms: string[]): RiskyAliasLookup {
  const query = useQuery({
    queryKey: ["vocabulary-risky-aliases", terms],
    queryFn: () => fetchRiskyAliasLookup(terms),
    staleTime: Infinity,
  });

  return query.data ?? NICHTS_VERWORFEN;
}
