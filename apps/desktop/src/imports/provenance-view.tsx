import { Trans } from "@lingui/react/macro";
import { Clock, DownloadSimple } from "@phosphor-icons/react";

import { safeFormat, safeParseDate } from "@anlg/utils";

import {
  useSessionImportProvenance,
  useSessionTimingQuality,
} from "./provenance-queries";

/**
 * Die Quell-Kennungen liegen technisch in der Datenbank ("anarlog",
 * "microsoft-teams"). Am Gespraech soll der Name stehen, den der Mensch kennt.
 * Eine unbekannte Kennung wird unveraendert gezeigt statt verschwiegen: falsch
 * beschriftet ist schlimmer, gar nicht beschriftet ist schlimmer als roh.
 */
const SOURCE_APP_NAMES: Record<string, string> = {
  anarlog: "anarlog / Hyprnote",
  hyprnote: "anarlog / Hyprnote",
  "chatgpt-record": "ChatGPT Record",
  "google-meet": "Google Meet",
  "microsoft-teams": "Microsoft Teams",
  "read-ai": "Read AI",
  "slack-huddles": "Slack Huddles",
  "clari-copilot": "Clari Copilot",
  fireflies: "Fireflies.ai",
  otter: "Otter.ai",
  tldv: "tl;dv",
};

export function importSourceLabel(sourceApp: string | null): string | null {
  if (!sourceApp) return null;
  return SOURCE_APP_NAMES[sourceApp] ?? sourceApp;
}

function formatImportedAt(importedAt: string | null): string | null {
  const parsed = importedAt ? safeParseDate(importedAt) : null;
  return parsed ? safeFormat(parsed, "MMM d, yyyy") : null;
}

/** Kleiner Merker fuer die Gespraechsliste. Traegt Quelle und Datum im Titel. */
export function ImportedMarker({
  sourceApp,
  importedAt,
}: {
  sourceApp: string | null;
  importedAt: string | null;
}) {
  const source = importSourceLabel(sourceApp);
  const day = formatImportedAt(importedAt);
  const title = source
    ? day
      ? `Imported from ${source} on ${day}`
      : `Imported from ${source}`
    : "Imported";

  return (
    <span
      title={title}
      aria-label={title}
      className="border-border text-muted-foreground inline-flex shrink-0 items-center gap-0.5 rounded border px-1 text-[10px] leading-4"
    >
      <DownloadSimple className="size-2.5" aria-hidden="true" />
      <Trans>Imported</Trans>
    </span>
  );
}

/** Der ganze Satz, fuer die Sitzungsansicht. */
export function SessionImportNote({ sessionId }: { sessionId: string }) {
  const provenance = useSessionImportProvenance(sessionId);
  if (!provenance) return null;

  const source = importSourceLabel(provenance.sourceApp);
  const day = formatImportedAt(provenance.importedAt);

  return (
    <div className="text-muted-foreground flex items-start gap-2 text-xs">
      <DownloadSimple className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span className="min-w-0">
        {source && day ? (
          <Trans>
            Brought over from {source} on {day}.
          </Trans>
        ) : source ? (
          <Trans>Brought over from {source}.</Trans>
        ) : (
          <Trans>Brought over from another app.</Trans>
        )}
        {provenance.sourceRoot ? (
          <span className="mt-0.5 block break-all opacity-70">
            {provenance.sourceRoot}
          </span>
        ) : null}
      </span>
    </div>
  );
}

/**
 * Steht ueber dem Transkript, wenn die Wortzeiten geschaetzt sind. Ohne diesen
 * Satz springt der Abspieler daneben und der Mensch haelt das fuer einen Fehler
 * der App, nicht fuer eine Eigenschaft der Quelle.
 */
export function TranscriptTimingNotice({ sessionId }: { sessionId: string }) {
  const quality = useSessionTimingQuality(sessionId);
  if (quality !== "synthetic" && quality !== "mixed") return null;

  return (
    <div
      data-testid="transcript-timing-notice"
      className="border-border bg-muted/50 text-muted-foreground flex items-start gap-2 border-b px-3 py-2 text-xs"
    >
      <Clock className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <span>
        {quality === "synthetic" ? (
          <Trans>
            The word times in this transcript are estimated, not measured.
            Clicking a word will not land on it exactly in the audio.
          </Trans>
        ) : (
          <Trans>
            Some of the word times in this transcript are estimated, not
            measured. Clicking a word there will not land on it exactly in the
            audio.
          </Trans>
        )}
      </span>
    </div>
  );
}
