/**
 * Das Postfach fuer Vorschlaege aus den beiden Netzen.
 *
 * WARUM HIER und nicht in `session_proposals`: die vorhandene Tabelle ist ein
 * Postfach fuer MARKDOWN-Diffs an EINEM Gespraech (`current_markdown` /
 * `proposed_markdown`, Spalte `session_id`), und sie haengt an der
 * Ueberpruefungs-Oberflaeche im Chat- und Bearbeiten-Tab. Ein Wortpaar ist
 * kein Markdown-Diff, und Netz 1 findet sein Signal gerade NICHT in einem
 * einzelnen Gespraech, sondern ueber den Stapel -- es gaebe keine `session_id`,
 * unter die der Vorschlag gehoert.
 *
 * Angenommen wird er ausserdem nicht in ein Gespraech hinein, sondern ins
 * Woerterbuch. Also gehoert er dorthin, wo das Woerterbuch steht: neben die
 * Liste, in die er faellt, mit denselben Waechtern daneben.
 */

import { Trans, useLingui } from "@lingui/react/macro";
import { Check, MagnifyingGlass, X } from "@phosphor-icons/react";

import { Button } from "@anlg/ui/components/ui/button";

import {
  proposalKey,
  useProposals,
  type VocabularyProposal,
} from "~/stt/proposals";

export function DictionaryProposals({
  terms,
  dismissed,
  running,
  onStart,
  onAccept,
  onDismiss,
}: {
  terms: string[];
  dismissed: string[];
  running: boolean;
  onStart: () => void;
  onAccept: (proposal: VocabularyProposal) => void;
  onDismiss: (proposal: VocabularyProposal) => void;
}) {
  const { t } = useLingui();
  const query = useProposals({ terms, dismissed, enabled: running });
  const proposals = query.data ?? [];

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-3 px-4">
        <div className="flex min-w-0 flex-col">
          <span className="text-sm font-medium">
            <Trans>Suggested mishearings</Trans>
          </span>
          <span className="text-muted-foreground text-xs">
            <Trans>
              Found in your existing transcripts. Nothing is changed until you
              accept.
            </Trans>
          </span>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="shrink-0"
          onClick={() => {
            // Beim ersten Klick anschalten, bei jedem weiteren neu laufen
            // lassen. Ohne das `refetch` waere der zweite Klick ein Knopf,
            // der nichts tut: `enabled` ist schon true und `staleTime` steht
            // auf Unendlich -- neue Transkripte blieben ungesehen, und die
            // Oberflaeche sagte es nicht.
            if (running) {
              void query.refetch();
            } else {
              onStart();
            }
          }}
          disabled={query.isFetching}
        >
          <MagnifyingGlass className="size-4" />
          {query.isFetching ? (
            <Trans>Searching…</Trans>
          ) : (
            <Trans>Search suggestions</Trans>
          )}
        </Button>
      </div>

      {/* Ein Ausfall darf nie wie ein sauberes Ergebnis aussehen. Leer und
          unlesbar sind zwei verschiedene Aussagen, und nur eine davon heisst
          "es gibt nichts zu tun". */}
      {query.isError && !query.isFetching && (
        <p className="px-4 text-sm text-yellow-600 dark:text-yellow-500">
          <Trans>
            The search could not be completed, so this is not a result. Please
            try again.
          </Trans>
        </p>
      )}

      {running &&
        !query.isFetching &&
        !query.isError &&
        proposals.length === 0 && (
          <p className="text-muted-foreground px-4 text-sm">
            <Trans>Nothing found in your transcripts.</Trans>
          </p>
        )}

      {proposals.length > 0 && (
        <div className="border-border bg-card divide-border divide-y overflow-hidden rounded-2xl border">
          {proposals.map((proposal) => {
            const key = proposalKey(proposal.canonical, proposal.alias);
            return (
              <div
                key={key}
                className="flex min-h-12 items-start justify-between gap-3 py-3 pr-3 pl-4"
              >
                <div className="flex min-w-0 flex-col gap-1">
                  <span className="text-sm">
                    {t`${proposal.alias} → ${proposal.canonical}`}
                  </span>
                  <span className="text-muted-foreground text-xs break-words">
                    {/* Die Zahl traegt ihre Stueckzahl mit: "1x in einem
                        Gespraech" ist ein schwaecherer Beleg als "5x in drei",
                        und der Mensch soll das sehen, bevor er entscheidet. */}
                    {t`${proposal.occurrences}x in ${proposal.sessionIds.length} conversation(s) · ${proposal.source}`}
                  </span>
                  <span className="text-muted-foreground/80 text-xs break-words italic">
                    …{proposal.evidence}
                  </span>
                </div>
                <div className="flex shrink-0 items-center gap-1">
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="text-muted-foreground hover:text-foreground size-7"
                    onClick={() => onDismiss(proposal)}
                    aria-label={t`Dismiss ${proposal.alias}`}
                  >
                    <X className="size-4" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    className="text-muted-foreground hover:text-foreground size-7"
                    onClick={() => onAccept(proposal)}
                    aria-label={t`Accept ${proposal.alias}`}
                  >
                    <Check className="size-4" />
                  </Button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
