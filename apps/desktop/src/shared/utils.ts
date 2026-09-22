import { getIdentifier } from "@tauri-apps/api/app";

// export * from "../shared/config/configure-pro-settings";
// export * from "~/sidebar/timeline/utils";
// export * from "~/stt/segment";

export const id = () => crypto.randomUUID() as string;

export type DesktopScheme =
  | "mitschnitt"
  | "anarlog"
  | "anarlog-staging"
  | "anarlog-dev";

// Fork: der Rueckkanal darf NIE bei einer fremden Installation landen.
// Upstream mappte jede unbekannte Identitaet auf "anarlog" -- bei uns fiel damit
// JEDER Mitschnitt-Build (media.zickert.mitschnitt*) auf ein Schema zurueck, das
// auf dem Rechner des Prinzipals seiner produktiven anarlog-Installation gehoert.
// Ein Auth-/Billing-Ruecksprung waere dort aufgeschlagen, nicht bei uns.
// Registriert ist "mitschnitt" (tauri.conf.json "schemes"), also ist das der
// einzig richtige Wert - und der Fallback gehoert auf die EIGENE Identitaet,
// nicht auf die fremde. Gleiche Klasse wie der bereits geschlossene
// Datenordner-Fallback (crates/storage/src/global.rs, embedded_cli.rs:218-227),
// nur ueber die Schema-Achse. Gefunden im Zweitblick zum v1.4.14-Sprung.
const DEFAULT_SCHEME: DesktopScheme = "mitschnitt";

export const getScheme = async (): Promise<DesktopScheme> => {
  const id = await getIdentifier();
  const schemes: Record<string, DesktopScheme> = {
    "media.zickert.mitschnitt": "mitschnitt",
    "media.zickert.mitschnitt.stable": "mitschnitt",
    "media.zickert.mitschnitt.staging": "mitschnitt",
    "media.zickert.mitschnitt.desktop": "mitschnitt",
    "media.zickert.mitschnitt.flatpak": "mitschnitt",
  };
  return schemes[id] ?? DEFAULT_SCHEME;
};

// https://www.rfc-editor.org/rfc/rfc4122#section-4.1.7
export const DEFAULT_USER_ID = "00000000-0000-0000-0000-000000000000";
