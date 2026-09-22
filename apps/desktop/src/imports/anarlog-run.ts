import {
  getImportRun,
  runSourceImport,
  type ImportRunReport,
} from "./anarlog-contract";

export const IMPORT_RUN_POLL_MS = 600;

/**
 * Startet einen Lauf und fragt seinen Bericht ab, bis er nicht mehr laeuft.
 *
 * Bewusst OHNE Zeitlimit: ein Bestand mit tausenden Gespraechen und Gigabyte an
 * Ton laeuft legitim lange, und ein Limit, das dabei zuschlaegt, meldet einen
 * Fehlschlag fuer einen Import, der gerade arbeitet. Der Ausweg des Menschen ist
 * stattdessen, dass der Dialog waehrenddessen schliessbar bleibt.
 *
 * `onProgress` bekommt AUCH den letzten Bericht. Ohne das haengt die Anzeige bei
 * der vorletzten Zahl fest, waehrend daneben schon "fertig" steht.
 */
export async function runImportWithProgress(
  args: { sourcePath: string; dryRun: boolean; copyAudio: boolean },
  options: {
    onProgress?: (report: ImportRunReport) => void;
    deps?: {
      start: typeof runSourceImport;
      read: typeof getImportRun;
      wait: (ms: number) => Promise<void>;
    };
  } = {},
): Promise<ImportRunReport> {
  const start = options.deps?.start ?? runSourceImport;
  const read = options.deps?.read ?? getImportRun;
  const wait = options.deps?.wait ?? defaultWait;

  const runId = await start(args.sourcePath, args.dryRun, args.copyAudio);

  for (;;) {
    const report = await read(runId);
    options.onProgress?.(report);
    if (report.status !== "running") return report;
    await wait(IMPORT_RUN_POLL_MS);
  }
}

function defaultWait(ms: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, ms));
}
