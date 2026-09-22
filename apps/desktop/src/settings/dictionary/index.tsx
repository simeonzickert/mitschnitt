import { Trans, useLingui } from "@lingui/react/macro";
import {
  BookOpen,
  Check,
  MinusCircle,
  PencilSimple,
  Plus,
  Warning,
  X,
} from "@phosphor-icons/react";
import { useForm } from "@tanstack/react-form";
import { type KeyboardEvent, useState } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import { Input } from "@anlg/ui/components/ui/input";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@anlg/ui/components/ui/input-group";

import { DictionaryProposals } from "./proposals";
import { beobachteSchreibvorgang, useSchreibfehler } from "./write-status";

import { SettingsPageTitle } from "~/settings/page-title";
import { updateSettingValue } from "~/settings/queries";
import { parseConfigStringList, useConfigValue } from "~/shared/config";
import {
  addDictionaryAlias,
  type DictionaryEntry,
  dictionaryEntryKey,
  formatDictionaryEntry,
  parseAliasInput,
  parseDictionaryEntries,
  parseDictionaryEntry,
  replaceDictionaryEntry,
} from "~/stt/dictionary-entry";
import { normalizeKeywordList, parseDictionaryTermsText } from "~/stt/keywords";
import { proposalKey } from "~/stt/proposals";
import { useRiskyAliases } from "~/stt/risky-aliases";

export function SettingsDictionary() {
  const terms = useConfigValue("personalization_dictionary_terms");
  const dismissed = useConfigValue("personalization_dictionary_dismissed");
  const [suche, setSuche] = useState(false);
  const fehler = useSchreibfehler();

  const liste = listenSchreiber("personalization_dictionary_terms");
  const verworfen = listenSchreiber("personalization_dictionary_dismissed");

  const merkeVerworfen = (proposal: { canonical: string; alias: string }) => {
    const key = proposalKey(proposal.canonical, proposal.alias);
    return verworfen((jetzt) =>
      jetzt.includes(key) ? jetzt : [...jetzt, key],
    );
  };

  return (
    <div className="flex flex-col gap-8">
      <SettingsPageTitle title={<Trans>Dictionary</Trans>} />
      {/* Ein fehlgeschlagenes Speichern sah bis zum 03.09.2026 aus wie Erfolg:
          der Eintrag verschwand aus der Anzeige und kam beim naechsten
          Oeffnen zurueck. Ein Werkzeug, das schweigt, wenn es kaputt ist, ist
          schlimmer als eines, das gar nicht laeuft. */}
      {fehler && (
        <p className="px-4 text-sm text-yellow-600 dark:text-yellow-500">
          <Trans>
            The change could not be saved, so it did not take effect. Please try
            again.
          </Trans>
        </p>
      )}
      <DictionarySettings terms={terms} onSave={liste} />
      <DictionaryProposals
        terms={terms}
        dismissed={dismissed}
        running={suche}
        onStart={() => setSuche(true)}
        onAccept={(proposal) => {
          // Genau derselbe Weg wie eine Korrektur im Transkript: der Vorschlag
          // wird ein gewoehnlicher Alias, nicht ein Sonderfall daneben.
          //
          // DIE REIHENFOLGE IST DIE ZUSAGE. Bis zum 03.09.2026 liefen beide
          // Schreibvorgaenge nebeneinander los: erst der Alias, dann -- ohne
          // auf ihn zu warten -- das Merken als erledigt. Die Schlange
          // (`enqueueDatabaseWrite`) faehrt nach einem Fehlschlag ausdruecklich
          // weiter (`previous.catch(() => {})`), also lief der zweite auch
          // dann, wenn der erste gescheitert war. Ergebnis: KEIN Eintrag im
          // Woerterbuch, der Vorschlag aber dauerhaft verworfen und aus dem
          // Postfach verschwunden -- waehrend das Banner daneben sagte, es sei
          // nichts passiert. Das war der Weg, auf dem ein Mensch einen
          // Vorschlag verliert, den er ausdruecklich annehmen wollte.
          //
          // "Verworfen" haengt jetzt AM ERFOLG des Alias-Schreibvorgangs.
          // Scheitert der, wird nichts verworfen und der Vorschlag steht beim
          // naechsten Lauf wieder da.
          //
          // Die umgekehrte Richtung ist die billige und bleibt erlaubt: Alias
          // steht, Merken scheitert. Dann kommt der Vorschlag noch einmal --
          // und ihn ein zweites Mal anzunehmen aendert nichts, weil der Alias
          // schon da ist.
          void liste((jetzt) =>
            addDictionaryAlias(jetzt, proposal.canonical, proposal.alias),
          )
            .then(
              // Auch ein angenommener Vorschlag wird gemerkt. Sonst legt der
              // naechste Lauf ihn wieder vor -- und ein Postfach, das
              // Erledigtes zurueckbringt, liest irgendwann niemand mehr.
              () => merkeVerworfen(proposal),
              () => undefined,
            )
            .catch(() => undefined);
        }}
        onDismiss={(proposal) => {
          void merkeVerworfen(proposal).catch(() => undefined);
        }}
      />
    </div>
  );
}

/**
 * Rechnet die naechste Liste aus der Liste, die gerade WIRKLICH gespeichert ist.
 *
 * Gibt den Schreibvorgang HERAUS, und er scheitert sichtbar. Ein Aufrufer, der
 * danach noch etwas tun will -- ein Eingabefeld leeren, einen Vorschlag als
 * erledigt merken -- muss wissen, ob das Schreiben geklappt hat. Solange das
 * hier `void` war, tat jeder Aufrufer so, als sei es immer gutgegangen.
 *
 * Wer das Ergebnis nicht braucht, schreibt `void schreiber(...).catch(() =>
 * undefined)`: der Fehler steht ohnehin im Banner, und eine unbehandelte
 * Ablehnung waere ein zweiter Meldeweg, der nichts Neues sagt.
 */
export type ListenSchreiber = (
  rechne: (aktuell: string[]) => string[],
) => Promise<void>;

/**
 * Der Schreibweg fuer eine Listen-Einstellung, der sich selbst liest.
 *
 * Das Problem, gegen das das steht: die gespeicherte Einstellung zieht erst
 * einen Rundlauf spaeter nach. Klickt jemand A und sofort danach B, hat der
 * zweite Klick den ersten noch nicht gesehen -- A geht verloren und kommt beim
 * naechsten Lauf wieder. Die Zusage "Verwerfen ist dauerhaft" waere still
 * gebrochen.
 *
 * Zwei Versuche standen hier vorher, beide mit einem optimistischen Vorgriff
 * neben dem gespeicherten Stand -- erst als VEREINIGUNG, dann als ERSATZ. Der
 * Ersatz gab sich erst frei, wenn der gespeicherte Stand dem selbst
 * geschriebenen EXAKT entsprach, und genau daran starb er: die Live-Abfrage
 * buendelt Tabellenaenderungen (`crates/db-reactive/src/runtime.rs`), eine
 * fremde Aenderung kann den Zwischenstand ueberspringen, und dann wird der
 * Vorgriff NIE mehr freigegeben. Der naechste Klick rechnet weiter gegen den
 * eigenen alten Stand und schreibt die fremde Aenderung dauerhaft weg. Der
 * eingeraeumte Preis war "ein Rundlauf" -- er war in Wahrheit unbegrenzt, mit
 * Datenverlust.
 *
 * Der Vorgriff ist deshalb GANZ WEG. Die Abwaegung, offen:
 *
 * - Was er brachte, war Sofort-Anzeige. Was er kostete, waren zwei Klassen
 *   stiller Datenverluste in drei Runden -- die Sorte Bauteil, die bei jeder
 *   Reparatur eine neue Luecke aufmacht.
 * - Der Ersatz kostet nichts. `updateSettingValue` liest den gespeicherten
 *   Stand INNERHALB der Schreib-Schlange (`enqueueDatabaseWrite`, Schluessel
 *   "app-settings") und schreibt daraus. Zwei schnelle Klicks laufen
 *   nacheinander durch dieselbe Schlange, der zweite sieht den ersten, und
 *   eine fremde Aenderung dazwischen wird gelesen statt ueberschrieben.
 * - Was bleibt, ist die Anzeige-Verzoegerung von einem Rundlauf. Das ist eine
 *   Sekunde Geduld gegen eine Klasse von Verlusten -- und der einzige Preis,
 *   den dieser Tausch wirklich hat.
 *
 * Der Rueckgabewert traegt ausserdem den Fehler nach oben. Vorher verschluckte
 * `useSetSettingValue` ihn in ein `console.error`.
 */
// Bewusst KEIN Hook und deshalb ohne "use"-Namen: die Funktion ruft nichts aus
// React auf. Ein Hook-Name wuerde einem spaeteren Leser Regeln versprechen, die
// hier nicht gelten.
function listenSchreiber(
  key:
    | "personalization_dictionary_terms"
    | "personalization_dictionary_dismissed",
): ListenSchreiber {
  return (rechne) =>
    beobachteSchreibvorgang(
      updateSettingValue(key, (gespeichert) =>
        JSON.stringify(rechne(parseConfigStringList(gespeichert))),
      ),
    ).then(
      () => undefined,
      (error: unknown) => {
        console.error(`[dictionary] failed to update ${key}`, error);
        throw error;
      },
    );
}

export function DictionarySettings({
  terms,
  onSave,
}: {
  /** Der gespeicherte Stand, fuer die Anzeige. */
  terms: string[];
  /**
   * Schreibt, und rechnet dabei gegen den Stand im Moment des SCHREIBENS.
   *
   * Zwei schnelle Klicks auf zwei Loeschknoepfe rechneten frueher beide gegen
   * dieselbe Momentaufnahme -- der zweite machte den ersten rueckgaengig.
   */
  onSave: ListenSchreiber;
}) {
  const { t } = useLingui();
  const entries = parseDictionaryEntries(terms);
  const normalizedTerms = entries.map(formatDictionaryEntry);
  const normalisiere = (liste: string[]) =>
    parseDictionaryEntries(liste).map(formatDictionaryEntry);
  // Die Entscheidung kommt aus Rust, nicht aus einer Kopie hier.
  const istVerworfen = useRiskyAliases(normalizedTerms);

  const form = useForm({
    defaultValues: {
      term: "",
      aliases: "",
    },
    onSubmit: async ({ value }) => {
      // Die Vorpruefung laeuft gegen die ANZEIGE, weil sie nur entscheidet, ob
      // die Eingabefelder geleert werden -- den Eingabetext eines Menschen
      // wegzuwerfen, ohne dass etwas passiert ist, waere der teurere Fehler.
      // Was WIRKLICH geschrieben wird, rechnet der Schreiber selbst neu.
      if (
        appendDictionaryTerms(normalizedTerms, value.term, value.aliases).join(
          "\n",
        ) === normalizedTerms.join("\n")
      ) {
        return;
      }

      // Erst schreiben, DANN leeren. Bis zum 03.09.2026 wurden die Felder
      // sofort geraeumt, und ein gescheiterter Schreibvorgang nahm den
      // getippten Text mit -- der Mensch sah ein Banner "bitte noch einmal
      // versuchen" ueber zwei leeren Feldern und musste alles neu tippen.
      try {
        await onSave((jetzt) =>
          appendDictionaryTerms(normalisiere(jetzt), value.term, value.aliases),
        );
      } catch {
        return;
      }
      form.setFieldValue("term", "");
      form.setFieldValue("aliases", "");
    },
  });

  const removeTerm = (term: string) => {
    void onSave((jetzt) =>
      normalisiere(jetzt).filter((value) => value !== term),
    ).catch(() => undefined);
  };

  /**
   * Ersetzt GENAU den Eintrag mit diesem Namen -- gerechnet gegen den Stand
   * im Moment des SCHREIBENS, nicht gegen die Anzeige. Reicht den Fehlschlag
   * an den Aufrufer weiter: die Zeile bleibt im Bearbeitungsmodus stehen,
   * bis das Speichern wirklich durch ist -- derselbe Grundsatz wie beim
   * Hinzufuegen ("Erst schreiben, DANN leeren").
   */
  const editEntry = (original: DictionaryEntry, nextEntry: DictionaryEntry) =>
    onSave((jetzt) =>
      replaceDictionaryEntry(
        jetzt,
        dictionaryEntryKey(original.canonical),
        nextEntry,
      ),
    );

  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        event.stopPropagation();
        void form.handleSubmit();
      }}
    >
      <div className="flex flex-col gap-2">
        <InputGroup className="border-border bg-card has-[[data-slot=input-group-control]:focus-visible]:border-border rounded-full shadow-none has-[[data-slot=input-group-control]:focus-visible]:ring-0">
          <form.Field name="term">
            {(field) => (
              <InputGroupInput
                className="pr-4 pl-4"
                aria-label={t`Term`}
                placeholder={t`Add names, jargon, or product terms to prefer`}
                value={field.state.value}
                onChange={(event) => field.handleChange(event.target.value)}
                onBlur={field.handleBlur}
              />
            )}
          </form.Field>
          <InputGroupAddon align="inline-end">
            <form.Subscribe
              selector={(state) => [state.values.term, state.values.aliases]}
            >
              {([term, aliases]) => {
                const canAdd =
                  appendDictionaryTerms(normalizedTerms, term, aliases).join(
                    "\n",
                  ) !== normalizedTerms.join("\n");

                return (
                  <InputGroupButton
                    type="submit"
                    variant="ghost"
                    size="xs"
                    className="rounded-full bg-black text-white hover:bg-black/90 hover:text-white dark:bg-white dark:text-black dark:hover:bg-white/90 dark:hover:text-black"
                    disabled={!canAdd}
                    aria-label={t`Add`}
                  >
                    <Plus className="size-3.5" />
                    <Trans>Add</Trans>
                  </InputGroupButton>
                );
              }}
            </form.Subscribe>
          </InputGroupAddon>
        </InputGroup>

        <InputGroup className="border-border bg-card has-[[data-slot=input-group-control]:focus-visible]:border-border rounded-full shadow-none has-[[data-slot=input-group-control]:focus-visible]:ring-0">
          <form.Field name="aliases">
            {(field) => (
              <InputGroupInput
                className="pr-4 pl-4"
                aria-label={t`Misheard as`}
                placeholder={t`Misheard as (optional, separate with commas)`}
                value={field.state.value}
                onChange={(event) => field.handleChange(event.target.value)}
                onBlur={field.handleBlur}
              />
            )}
          </form.Field>
        </InputGroup>

        <p className="text-muted-foreground px-4 text-xs">
          <Trans>
            A misheard spelling is what transcription actually produced. Without
            one, the term is only a hint to the model, and nothing in a finished
            transcript gets corrected.
          </Trans>
        </p>
      </div>

      <form.Subscribe selector={(state) => state.values.term}>
        {(value) => {
          const visibleEntries = getVisibleDictionaryEntries(entries, value);
          const hasSearch = parseDictionaryTermsText(value).length > 0;

          if (entries.length === 0) {
            return (
              <div className="border-border bg-card flex min-h-40 flex-col items-center justify-center rounded-2xl border px-6 text-center">
                <BookOpen className="text-muted-foreground mb-3 size-5" />
                <p className="text-sm font-medium">
                  <Trans>Your dictionary is empty</Trans>
                </p>
                <p className="text-muted-foreground mt-1 max-w-sm text-xs">
                  <Trans>
                    Tip: Add teammate names, acronyms, company jargon, and
                    product terms.
                  </Trans>
                </p>
              </div>
            );
          }

          if (visibleEntries.length === 0) {
            return hasSearch ? (
              <p className="text-muted-foreground px-4 text-sm">
                <Trans>No match</Trans>
              </p>
            ) : null;
          }

          return (
            <div className="border-border bg-card divide-border divide-y overflow-hidden rounded-2xl border">
              {visibleEntries.map((entry) => (
                <DictionaryEntryRow
                  // Formatierte Zeile statt Roh-Canonical: zwei Eintraege mit
                  // gleichem Namen kann es nach `parseDictionaryEntries` nicht
                  // geben, aber die Zeile bleibt der stabile Schluessel, den
                  // auch `removeTerm` schon benutzt.
                  key={formatDictionaryEntry(entry)}
                  entry={entry}
                  // Der VOLLE Bestand, nicht die gefilterte Anzeige -- die
                  // Kollisionspruefung beim Bearbeiten muss jeden anderen
                  // Namen kennen, auch den, den die Suche gerade ausblendet.
                  entries={entries}
                  // Der Nachlauf wendet eine Verhoerung nicht an, die selbst
                  // gewoehnliches Deutsch ist -- bis hierher stand das nur im
                  // Protokoll, und der Mensch sah einen Eintrag, der aussah,
                  // als wuerde er wirken.
                  ignored={entry.aliases.filter((alias) =>
                    istVerworfen(entry.canonical, alias),
                  )}
                  onEdit={editEntry}
                  onRemove={removeTerm}
                />
              ))}
            </div>
          );
        }}
      </form.Subscribe>
    </form>
  );
}

function DictionaryEntryRow({
  entry,
  entries,
  ignored,
  onEdit,
  onRemove,
}: {
  entry: DictionaryEntry;
  /**
   * Der volle Bestand, nicht die per Suche gefilterte Anzeige -- die
   * Kollisionspruefung beim Bearbeiten muss jeden anderen Namen kennen.
   */
  entries: DictionaryEntry[];
  ignored: string[];
  onEdit: (
    original: DictionaryEntry,
    nextEntry: DictionaryEntry,
  ) => Promise<void>;
  onRemove: (line: string) => void;
}) {
  const { t } = useLingui();
  const line = formatDictionaryEntry(entry);
  // Heisst `term`, weil der Uebersetzungsschluessel daran haengt: siehe die
  // Erklaerung bei "Remove" weiter unten -- derselbe Grund gilt hier fuer
  // "Edit".
  const term = entry.canonical;

  const [bearbeitung, setBearbeitung] = useState<{
    term: string;
    aliases: string;
  } | null>(null);

  // Rechnet gegen die ANZEIGE, weil sie nur entscheidet, ob der Save-Knopf
  // gedrueckt werden darf. Was WIRKLICH geschrieben wird, rechnet `onEdit`
  // (und darunter `replaceDictionaryEntry`) beim Schreiben selbst noch
  // einmal nach -- derselbe Grundsatz wie bei `appendDictionaryTerms` oben.
  const nextEntry =
    bearbeitung === null
      ? null
      : getEditedDictionaryEntry(
          entries,
          entry,
          bearbeitung.term,
          bearbeitung.aliases,
        );

  const beginEdit = () =>
    setBearbeitung({
      term: entry.canonical,
      aliases: entry.aliases.join(", "),
    });

  const saveEdit = async () => {
    if (!nextEntry) return;
    try {
      await onEdit(entry, nextEntry);
    } catch {
      // Bleibt im Bearbeitungsmodus stehen, mit der getippten Eingabe --
      // derselbe Grundsatz wie beim Hinzufuegen: "Erst schreiben, DANN
      // leeren". Ein gescheitertes Speichern darf die Eingabe nicht mitnehmen.
      return;
    }
    setBearbeitung(null);
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      void saveEdit();
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setBearbeitung(null);
    }
  };

  if (bearbeitung !== null) {
    return (
      <div className="flex min-h-12 flex-col gap-2 py-3 pr-3 pl-4">
        <Input
          autoFocus
          className="h-8"
          value={bearbeitung.term}
          onChange={(event) =>
            setBearbeitung({ ...bearbeitung, term: event.target.value })
          }
          onKeyDown={handleKeyDown}
          aria-label={t`Edit ${term}`}
        />
        <Input
          className="h-8"
          value={bearbeitung.aliases}
          onChange={(event) =>
            setBearbeitung({ ...bearbeitung, aliases: event.target.value })
          }
          onKeyDown={handleKeyDown}
          aria-label={t`Misheard as ${term}`}
        />
        <div className="flex justify-end gap-1">
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="text-muted-foreground hover:text-foreground size-7 shrink-0"
            onClick={() => void saveEdit()}
            disabled={!nextEntry}
            aria-label={t`Save`}
          >
            <Check className="size-4" />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon"
            className="text-muted-foreground hover:text-foreground size-7 shrink-0"
            onClick={() => setBearbeitung(null)}
            aria-label={t`Cancel`}
          >
            <X className="size-4" />
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="group flex min-h-12 items-start justify-between gap-3 py-3 pr-3 pl-4">
      <div className="flex min-w-0 flex-col gap-1">
        <span className="text-sm">{term}</span>
        {entry.aliases.length > 0 && (
          <span className="text-muted-foreground text-xs break-words">
            {t`Misheard as: ${entry.aliases.join(", ")}`}
          </span>
        )}
        {ignored.length > 0 && (
          <span className="flex items-start gap-1.5 text-xs text-yellow-600 dark:text-yellow-500">
            <Warning className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
            <span className="break-words">
              {t`Ignored, because it is ordinary German: ${ignored.join(
                ", ",
              )}`}
            </span>
          </span>
        )}
      </div>
      <div className="flex items-center">
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="text-muted-foreground hover:text-foreground size-7 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
          onClick={beginEdit}
          aria-label={t`Edit ${term}`}
        >
          <PencilSimple className="size-4" />
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="text-muted-foreground hover:text-foreground size-7 shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
          onClick={() => onRemove(line)}
          aria-label={t`Remove ${term}`}
        >
          <MinusCircle className="size-4" />
        </Button>
      </div>
    </div>
  );
}

/**
 * Was aus den beiden Bearbeitungsfeldern wuerde, wenn jetzt gespeichert
 * wird -- oder `null`, wenn nichts zu speichern ist: leerer oder mehrfacher
 * Name, keine Aenderung, oder eine Kollision mit einem ANDEREN vorhandenen
 * Eintrag.
 *
 * Das Namensfeld nimmt hier bewusst nur EINEN Namen: `parseDictionaryTermsText`
 * teilt an Komma und Zeilenumbruch, weil das Feld im Hinzufuegen-Formular oben
 * mehrere Namen auf einmal erlaubt -- beim Bearbeiten eines EINEN Eintrags
 * waere ein zweiter erkannter Name kein Tippfehler, den man raten sollte,
 * sondern ein Zeichen, dass das Feld nicht speicherbereit ist.
 */
function getEditedDictionaryEntry(
  entries: DictionaryEntry[],
  original: DictionaryEntry,
  termValue: string,
  aliasValue: string,
): DictionaryEntry | null {
  const canonicalCandidates = parseDictionaryTermsText(termValue);
  if (canonicalCandidates.length !== 1) {
    return null;
  }

  const nextEntry = parseDictionaryEntry(
    formatDictionaryEntry({
      canonical: canonicalCandidates[0],
      aliases: parseAliasInput(aliasValue),
    }),
  );
  if (!nextEntry) {
    return null;
  }
  if (formatDictionaryEntry(nextEntry) === formatDictionaryEntry(original)) {
    return null;
  }

  const originalKey = dictionaryEntryKey(original.canonical);
  const nextKey = dictionaryEntryKey(nextEntry.canonical);
  const collides = entries.some(
    (candidate) =>
      dictionaryEntryKey(candidate.canonical) !== originalKey &&
      dictionaryEntryKey(candidate.canonical) === nextKey,
  );

  return collides ? null : nextEntry;
}

/**
 * Was im Namensfeld steht, wird an Komma und Zeilenumbruch geteilt -- mehrere
 * Namen auf einmal. Verhoerungen gehoeren dann NICHT dazu: sie waeren nicht
 * zuzuordnen. Sie gelten nur, wenn genau ein Name eingegeben wird.
 */
function appendDictionaryTerms(
  terms: string[],
  value: string,
  aliasValue = "",
): string[] {
  const added = parseDictionaryTermsText(value);
  const aliases = added.length === 1 ? parseAliasInput(aliasValue) : [];

  // Anhaengen und zusammenlegen erledigt `parseDictionaryEntries` -- dieselbe
  // Regel wie `Vocabulary::parse`. Ein eigenes `Map.set` an dieser Stelle
  // haette bei zwei Zeilen desselben Namens die Verhoerungen der ersten
  // weggeworfen, und zwar beim Hinzufuegen eines voellig anderen Begriffs.
  const angehaengt = added.map((canonical) =>
    formatDictionaryEntry({ canonical, aliases }),
  );

  return normalizeKeywordList(
    parseDictionaryEntries([...terms, ...angehaengt]).map(
      formatDictionaryEntry,
    ),
  );
}

function getVisibleDictionaryEntries(
  entries: DictionaryEntry[],
  value: string,
): DictionaryEntry[] {
  const queries = parseDictionaryTermsText(value).map((term) =>
    term.toLocaleLowerCase(),
  );
  if (queries.length === 0) {
    return entries;
  }

  return entries.filter((entry) => {
    // Auch ueber die Verhoerungen: wer "Sarec" eintippt, will wissen, ob die
    // Verhoerung schon bekannt ist -- und bekam bis hierher "No match".
    const haystack = [entry.canonical, ...entry.aliases].map((text) =>
      text.toLowerCase(),
    );
    return queries.some((query) =>
      haystack.some((key) => key.includes(query) || query.includes(key)),
    );
  });
}
