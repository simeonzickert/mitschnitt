import type { MessageDescriptor } from "@lingui/core";
import { Trans, useLingui } from "@lingui/react/macro";
import { CircleNotch, FolderOpen, Warning } from "@phosphor-icons/react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { open as selectFolder } from "@tauri-apps/plugin-dialog";
import { useState } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import { Checkbox } from "@anlg/ui/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";

import {
  findImportSources,
  scanImportSource,
  type ImportRunReport,
  type ImportSourceScan,
} from "./anarlog-contract";
import { runImportWithProgress } from "./anarlog-run";
import {
  assessImportSource,
  formatBytes,
  formatSessionRange,
  importProblemMessage,
  requiredBytes,
} from "./anarlog-source";

/**
 * Eigener Ablauf fuer eine Quelle mit eigener Datenbank: Ordner waehlen, sehen
 * was drin ist, einen Probelauf lesen, DANN erst importieren. Der Probelauf ist
 * nicht optional -- der Knopf, der wirklich schreibt, erscheint erst, wenn sein
 * Bericht dasteht.
 */
export function AnarlogImportDialog({
  open,
  onOpenChange,
  providerName,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  providerName: string;
}) {
  const { t, i18n } = useLingui();
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [copyAudio, setCopyAudio] = useState(true);
  const [progress, setProgress] = useState<ImportRunReport | null>(null);
  const [finalReport, setFinalReport] = useState<ImportRunReport | null>(null);

  // Betreiber, 11.09.2026: „Niemand weiss, wo der Folder liegt." Deshalb sucht die
  // App beim Oeffnen selbst und zeigt, was sie hat -- der Auswahldialog bleibt
  // fuer Bestaende an ungewoehnlichen Orten.
  const gefundenQuery = useQuery({
    queryKey: ["anarlog-import", "gefunden"],
    queryFn: () => findImportSources(),
    enabled: open,
    staleTime: 0,
  });
  const gefunden = gefundenQuery.data ?? [];

  const scanQuery = useQuery({
    queryKey: ["anarlog-import", "scan", sourcePath],
    queryFn: () => scanImportSource(sourcePath!),
    enabled: Boolean(sourcePath) && !finalReport,
    retry: false,
    gcTime: 0,
  });

  const scan = scanQuery.data ?? null;
  const verdict = scan ? assessImportSource(scan, copyAudio) : null;
  const readyToTrial = Boolean(verdict?.canImport) && !finalReport;

  const dryRunQuery = useQuery({
    queryKey: ["anarlog-import", "dry-run", sourcePath, copyAudio],
    queryFn: () =>
      runImportWithProgress({
        sourcePath: sourcePath!,
        dryRun: true,
        copyAudio,
      }),
    enabled: readyToTrial,
    retry: false,
    gcTime: 0,
  });

  const importMutation = useMutation({
    mutationFn: () =>
      runImportWithProgress(
        { sourcePath: sourcePath!, dryRun: false, copyAudio },
        { onProgress: setProgress },
      ),
    onSuccess: setFinalReport,
  });

  const chooseFolder = useMutation({
    mutationFn: async () => {
      const selected = await selectFolder({
        title: t`Choose the folder with your old conversations`,
        directory: true,
        multiple: false,
      });
      return typeof selected === "string" ? selected : null;
    },
    onSuccess: (selected) => {
      if (!selected) return;
      setFinalReport(null);
      setProgress(null);
      setSourcePath(selected);
    },
  });

  const importing = importMutation.isPending;
  const dryRun = dryRunQuery.data ?? null;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // Aufgeraeumt wird beim Schliessen, nicht beim Oeffnen: sonst steht beim
        // naechsten Oeffnen fuer einen Wimpernschlag der alte Bericht da. Der
        // Aufrufer haengt den Dialog heute ohnehin ab, aber ein Dialog, der nur
        // sauber startet, WEIL sein Aufrufer ihn abhaengt, ist eine Falle fuer
        // den naechsten Aufrufer.
        if (!next) {
          setSourcePath(null);
          setProgress(null);
          setFinalReport(null);
          setCopyAudio(true);
          importMutation.reset();
          chooseFolder.reset();
        }
        onOpenChange(next);
      }}
    >
      <DialogContent className="max-w-lg gap-0 p-0">
        <DialogHeader className="gap-2 px-6 pt-6">
          <DialogTitle>
            <Trans>Bring your old conversations over</Trans>
          </DialogTitle>
          <DialogDescription>
            {/* Sobald die App selbst fuendig geworden ist, waere die
                Aufforderung "waehle den Ordner" falsch -- sie verlangte genau
                das, was gerade abgenommen wurde. Am Bildschirm gesehen,
                11.09.2026. */}
            {gefunden.length > 0 && !sourcePath ? (
              <Trans>
                Nothing is changed in that folder. Mitschnitt only reads it.
              </Trans>
            ) : (
              <Trans>
                Choose the folder that {providerName} keeps its data in. Nothing
                is changed in that folder. Mitschnitt only reads it.
              </Trans>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4 px-6 py-5 text-sm">
          {gefundenQuery.isPending && !sourcePath ? (
            <p className="text-muted-foreground flex items-center gap-2 text-xs">
              <CircleNotch className="size-3.5 animate-spin" />
              <Trans>Looking for your old conversations…</Trans>
            </p>
          ) : null}

          {!sourcePath && gefunden.length > 0 ? (
            <div className="flex flex-col gap-2">
              <p className="text-muted-foreground text-xs">
                <Trans>Found on this Mac:</Trans>
              </p>
              {gefunden.map((fund: ImportSourceScan) => (
                <button
                  key={fund.sourceRoot}
                  type="button"
                  className="hover:bg-muted flex flex-col items-start gap-0.5 rounded border px-3 py-2 text-left"
                  onClick={() => setSourcePath(fund.sourceRoot)}
                >
                  <span className="font-medium">
                    <Trans>
                      {fund.sessionCount} conversations from{" "}
                      {fund.sourceApp ?? providerName}
                    </Trans>
                  </span>
                  <span
                    className="text-muted-foreground w-full truncate text-xs"
                    title={fund.sourceRoot}
                  >
                    {fund.sourceRoot}
                  </span>
                </button>
              ))}
            </div>
          ) : null}

          {!sourcePath && !gefundenQuery.isPending && gefunden.length === 0 ? (
            <Explanation tone="advisory">
              <Trans>
                Nothing found in the usual places. If your old conversations
                live somewhere else, choose that folder below.
              </Trans>
            </Explanation>
          ) : null}

          <div className="flex items-center gap-3">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={chooseFolder.isPending || importing}
              onClick={() => chooseFolder.mutate()}
            >
              <FolderOpen className="size-3.5" />
              {sourcePath || gefunden.length > 0 ? (
                <Trans>Choose a different folder</Trans>
              ) : (
                <Trans>Choose folder</Trans>
              )}
            </Button>
            {sourcePath ? (
              <span
                className="text-muted-foreground min-w-0 flex-1 truncate text-xs"
                title={sourcePath}
              >
                {sourcePath}
              </span>
            ) : null}
          </div>

          {chooseFolder.error ? (
            <Explanation tone="problem">
              <Trans>
                The folder could not be opened: {chooseFolder.error.message}
              </Trans>
            </Explanation>
          ) : null}

          {sourcePath && scanQuery.isPending && !finalReport ? (
            <p className="text-muted-foreground flex items-center gap-2 text-xs">
              <CircleNotch className="size-3.5 animate-spin" />
              <Trans>Looking at this folder…</Trans>
            </p>
          ) : null}

          {scanQuery.error ? (
            <Explanation tone="problem">
              <Trans>
                This folder could not be read. Check whether it still exists and
                whether you are allowed to open it.
              </Trans>
            </Explanation>
          ) : null}

          {scan && verdict && !finalReport ? (
            <>
              {verdict.blockingProblems.map((key) => (
                <Explanation key={key} tone="problem">
                  {importProblemMessage(i18n, key)}
                </Explanation>
              ))}
              {verdict.advisoryProblems.map((key) => (
                <Explanation key={key} tone="advisory">
                  {importProblemMessage(i18n, key)}
                </Explanation>
              ))}
              {verdict.outOfSpace ? (
                <Explanation tone="problem">
                  <Trans>
                    This import needs about{" "}
                    {formatBytes(requiredBytes(scan, copyAudio))} and only{" "}
                    {formatBytes(scan.freeBytesOnTarget)} is free. Free some
                    space, or bring the conversations over without their audio.
                  </Trans>
                </Explanation>
              ) : null}

              {verdict.blockingProblems.length === 0 &&
              scan.sessionCount === 0 ? (
                <Explanation tone="problem">
                  <Trans>
                    There are no conversations in this folder. Choose the folder
                    that {providerName} stores its data in.
                  </Trans>
                </Explanation>
              ) : null}

              {scan.sessionCount > 0 ? (
                <SourceSummary
                  scan={scan}
                  locale={i18n.locale}
                  copyAudio={copyAudio}
                  onCopyAudioChange={setCopyAudio}
                  disabled={importing}
                />
              ) : null}

              {verdict.blockingProblems.length > 0 || verdict.outOfSpace ? (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="w-fit"
                  disabled={scanQuery.isFetching}
                  onClick={() => void scanQuery.refetch()}
                >
                  {scanQuery.isFetching ? (
                    <CircleNotch className="size-3.5 animate-spin" />
                  ) : null}
                  <Trans>Look again</Trans>
                </Button>
              ) : null}
            </>
          ) : null}

          {readyToTrial && dryRunQuery.isPending ? (
            <p className="text-muted-foreground flex items-center gap-2 text-xs">
              <CircleNotch className="size-3.5 animate-spin" />
              <Trans>Checking what would come over…</Trans>
            </p>
          ) : null}

          {dryRunQuery.error ? (
            <Explanation tone="problem">
              <Trans>
                The trial run did not get through, so nothing was imported:{" "}
                {dryRunQuery.error.message}
              </Trans>
            </Explanation>
          ) : null}

          {dryRun &&
          dryRun.status === "failed" &&
          !finalReport &&
          !importing ? (
            <Explanation tone="problem" testId="anarlog-dry-run">
              {importProblemMessage(i18n, dryRun.error ?? "")}
            </Explanation>
          ) : null}

          {dryRun &&
          dryRun.status !== "failed" &&
          !finalReport &&
          !importing ? (
            <Explanation tone="advisory" testId="anarlog-dry-run">
              <Trans>
                Trial run: of {dryRun.conversationsDiscovered} conversations,{" "}
                {dryRun.conversationsImported} would come over. Nothing has been
                changed yet.
              </Trans>
            </Explanation>
          ) : null}

          {importing ? (
            <p className="flex items-center gap-2 text-xs">
              <CircleNotch className="size-3.5 animate-spin" />
              <Trans>
                Bringing conversations over: {progress?.imported ?? 0} of{" "}
                {progress?.discovered ?? dryRun?.discovered ?? 0}
              </Trans>
            </p>
          ) : null}

          {importMutation.error ? (
            <Explanation tone="problem">
              <Trans>
                The import stopped partway: {importMutation.error.message}
              </Trans>
            </Explanation>
          ) : null}

          {finalReport ? (
            <Explanation
              tone={finalReport.status === "completed" ? "advisory" : "problem"}
              testId="anarlog-final-report"
            >
              <FinalReport report={finalReport} i18n={i18n} />
            </Explanation>
          ) : null}
        </div>

        <DialogFooter className="gap-2 px-6 pb-6">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            {finalReport ? <Trans>Done</Trans> : <Trans>Close</Trans>}
          </Button>
          {dryRun && !finalReport ? (
            <Button
              type="button"
              size="sm"
              disabled={importing || !verdict?.canImport}
              onClick={() => importMutation.mutate()}
            >
              {importing ? (
                <CircleNotch className="size-3.5 animate-spin" />
              ) : null}
              <Trans>Bring them over</Trans>
            </Button>
          ) : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourceSummary({
  scan,
  locale,
  copyAudio,
  onCopyAudioChange,
  disabled,
}: {
  scan: NonNullable<Awaited<ReturnType<typeof scanImportSource>>>;
  locale: string;
  copyAudio: boolean;
  onCopyAudioChange: (next: boolean) => void;
  disabled: boolean;
}) {
  const range = formatSessionRange(scan, locale);

  return (
    <div className="border-border bg-card flex flex-col gap-3 rounded-xl border px-4 py-3">
      <p>
        <Trans>
          Found {scan.sessionCount} conversations, {scan.transcriptCount}{" "}
          transcripts and {scan.documentCount} notes.
        </Trans>
      </p>
      {range ? (
        <p className="text-muted-foreground text-xs">
          <Trans>From {range}</Trans>
        </p>
      ) : null}
      <label className="flex items-start gap-2">
        <Checkbox
          checked={copyAudio}
          disabled={disabled}
          onCheckedChange={(next) => onCopyAudioChange(next === true)}
        />
        <span className="text-xs">
          <Trans>
            Copy the audio as well: {scan.audioFileCount} recordings,{" "}
            {formatBytes(scan.audioBytes)}
          </Trans>
        </span>
      </label>
      <p className="text-muted-foreground text-xs">
        <Trans>
          This needs about {formatBytes(requiredBytes(scan, copyAudio))} and{" "}
          {formatBytes(scan.freeBytesOnTarget)} is free.
        </Trans>
      </p>
    </div>
  );
}

function FinalReport({
  report,
  i18n,
}: {
  report: ImportRunReport;
  i18n: { _: (descriptor: MessageDescriptor) => string };
}) {
  if (report.status === "failed") {
    return (
      <span className="flex flex-col gap-1">
        <span>
          <Trans>
            The import stopped and did not finish.{" "}
            {report.conversationsImported} conversations had already come over
            before it stopped.
          </Trans>
        </span>
        {report.error ? (
          <span className="text-muted-foreground text-xs">
            {importProblemMessage(i18n, report.error)}
          </span>
        ) : null}
      </span>
    );
  }

  return (
    <span className="flex flex-col gap-1">
      <span>
        <Trans>{report.conversationsImported} conversations came over.</Trans>
      </span>
      {/* `conflicts` zaehlt Zeilen, die schon dastanden und mindestens so neu
          waren -- die Regel "die juengere Seite gewinnt" hat sie in Ruhe
          gelassen. Das ist der Normalfall und kein Befund. Bis 11.09.2026 stand
          hier "need a look": der erste echte Import meldete damit 1763
          angebliche Probleme, obwohl nichts schiefgegangen war. */}
      {report.conflicts > 0 ? (
        <span className="text-muted-foreground text-xs">
          <Trans>
            {report.conflicts} entries were already here and were left as they
            are.
          </Trans>
        </span>
      ) : null}
      {report.errors > 0 ? (
        <span>
          <Trans>{report.errors} could not be brought over.</Trans>
        </span>
      ) : null}
      {report.audioCopied > 0 ? (
        <span className="text-muted-foreground text-xs">
          <Trans>
            {report.audioCopied} recordings came with them,{" "}
            {formatBytes(report.audioBytesCopied)}.
          </Trans>
        </span>
      ) : null}
    </span>
  );
}

function Explanation({
  tone,
  testId,
  children,
}: {
  tone: "advisory" | "problem";
  testId?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      data-testid={testId}
      className={
        tone === "problem"
          ? "flex items-start gap-2 rounded-xl border border-yellow-300 bg-yellow-50 px-3 py-2 text-xs text-yellow-900"
          : "border-border bg-muted/50 text-muted-foreground flex items-start gap-2 rounded-xl border px-3 py-2 text-xs"
      }
    >
      {tone === "problem" ? (
        <Warning className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      ) : null}
      <span className="min-w-0">{children}</span>
    </div>
  );
}
