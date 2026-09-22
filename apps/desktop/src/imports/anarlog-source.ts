import type { MessageDescriptor } from "@lingui/core";
import { msg } from "@lingui/core/macro";

import type { ImportSourceScan } from "./anarlog-contract";

type I18n = { _: (descriptor: MessageDescriptor) => string };

/**
 * Kopfraum ueber dem, was der Ton selbst braucht. Ein Import, der die Platte bis
 * auf das letzte Byte fuellt, macht den Rechner unbenutzbar, lange bevor er
 * fehlschlaegt -- und die Datenbank waechst waehrend des Laufs mit.
 */
export const IMPORT_SPACE_HEADROOM_BYTES = 500 * 1024 * 1024;

/**
 * Die Schluessel kommen aus `ImportSourceScan.problems`. Jeder sagt, was los ist
 * UND was der Mensch dagegen tun kann -- ein Satz ohne Ausweg ist eine
 * Fehlermeldung mit besserer Grammatik.
 */
const problemMessages: Record<string, MessageDescriptor> = {
  no_data_found: msg`In this folder there are no conversations from another app. Choose the folder that the other app stores its data in.`,
  source_open: msg`The other app is running right now and is holding its data open. Quit it, then look again.`,
  not_enough_space: msg`There is not enough room on this disk for the audio. Free some space, or bring the conversations over without their audio.`,
  source_unreadable: msg`This folder cannot be read. Check whether it still exists and whether you are allowed to open it.`,
  same_installation: msg`This is Mitschnitt's own data folder. Everything in it is already here.`,
};

export function importProblemMessage(i18n: I18n, key: string): string {
  const known = problemMessages[key];
  if (known) return i18n._(known);
  return i18n._(
    msg`Something about this folder is in the way and this version does not have a name for it yet (${key}).`,
  );
}

/**
 * `not_enough_space` ist der einzige Schluessel, den der Mensch ohne neuen Scan
 * aufloesen kann: ohne Ton kopiert der Import fast nichts. Alle anderen bleiben
 * blockierend, unbekannte eingeschlossen -- bei einem Schluessel, den diese
 * Fassung nicht kennt, ist Nichtstun die sichere Richtung.
 */
export function isBlockingProblem(key: string, copyAudio: boolean): boolean {
  if (key === "not_enough_space") return copyAudio;
  return true;
}

export function requiredBytes(
  scan: Pick<ImportSourceScan, "audioBytes">,
  copyAudio: boolean,
): number {
  return (copyAudio ? scan.audioBytes : 0) + IMPORT_SPACE_HEADROOM_BYTES;
}

export function hasEnoughSpace(
  scan: Pick<ImportSourceScan, "audioBytes" | "freeBytesOnTarget">,
  copyAudio: boolean,
): boolean {
  return requiredBytes(scan, copyAudio) <= scan.freeBytesOnTarget;
}

export type ImportSourceVerdict = {
  canImport: boolean;
  blockingProblems: string[];
  advisoryProblems: string[];
  outOfSpace: boolean;
};

/**
 * Ein Scan, der nichts gefunden hat, aber auch kein `no_data_found` meldet, ist
 * trotzdem nichts zum Importieren. Die Oberflaeche verlaesst sich hier auf die
 * eigene Zaehlung und nicht darauf, dass die Gegenseite den Schluessel setzt.
 */
export function assessImportSource(
  scan: ImportSourceScan,
  copyAudio: boolean,
): ImportSourceVerdict {
  const blockingProblems = scan.problems.filter((key) =>
    isBlockingProblem(key, copyAudio),
  );
  const advisoryProblems = scan.problems.filter(
    (key) => !isBlockingProblem(key, copyAudio),
  );
  const outOfSpace = !hasEnoughSpace(scan, copyAudio);

  return {
    canImport:
      blockingProblems.length === 0 &&
      !outOfSpace &&
      scan.kind !== "none" &&
      scan.sessionCount > 0,
    blockingProblems,
    advisoryProblems,
    outOfSpace,
  };
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/**
 * Zeigt nur das Datum, nie die Uhrzeit: die Spanne beantwortet "ist das der
 * richtige Ordner", nicht "wann genau war das Gespraech".
 */
export function formatSessionRange(
  scan: Pick<ImportSourceScan, "oldestSessionAt" | "newestSessionAt">,
  locale: string | undefined,
): string | null {
  const oldest = parseDate(scan.oldestSessionAt);
  const newest = parseDate(scan.newestSessionAt);
  if (!oldest && !newest) return null;

  // Ein leeres oder unbrauchbares Gebietsschema wirft in Intl. Das darf hoechstens
  // die Datumsformatierung kosten, nie den ganzen Dialog.
  const formatter = dateFormatter(locale);
  const format = (value: Date) => formatter.format(value);
  if (!oldest) return format(newest!);
  if (!newest) return format(oldest);
  const from = format(oldest);
  const to = format(newest);
  return from === to ? from : `${from} – ${to}`;
}

function dateFormatter(locale: string | undefined): Intl.DateTimeFormat {
  try {
    return new Intl.DateTimeFormat(locale || undefined, {
      dateStyle: "medium",
    });
  } catch {
    return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
  }
}

function parseDate(value: string | null): Date | null {
  if (!value) return null;
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}
