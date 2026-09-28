import { commands as detectCommands } from "@anlg/plugin-detect";

import { hasKnownImportSource } from "./anarlog-contract";
import {
  detectMeetingImportProviders,
  type DetectedMeetingImportProvider,
  MEETING_IMPORT_PROVIDERS,
} from "./providers";

const ANARLOG_PROVIDER = MEETING_IMPORT_PROVIDERS.find(
  (provider) => provider.id === "anarlog",
);

/**
 * Die anarlog-Zeile erscheint nur, wenn auf diesem Rechner tatsaechlich ein
 * Bestand mit Gespraechen liegt -- geprueft ueber dieselbe Selbstsuche, die
 * auch der Importdialog nutzt.
 *
 * S3 (Orchestrator 26.09.2026): hier zaehlt nur die billige Ja/Nein-Frage
 * (`hasKnownImportSource`, kein Datenbank-Pool, kein Oeffnen der fremden
 * Datenbank, kein Zaehlen von Audio-Dateien) -- diese Funktion laeuft bei
 * jedem Oeffnen von Einstellungen oder Onboarding neu. Die teure, vollstaendige
 * Antwort (`findImportSources`) bleibt dem Importdialog vorbehalten, den ein
 * Mensch bewusst oeffnet. Ein Kollege, der die App nie hatte, sieht die Zeile
 * dadurch nirgends. Ein Fehlschlag der Suche wird als "nichts gefunden"
 * behandelt, nicht als Absturz der ganzen Erkennung.
 */
async function detectAnarlogFolderSource(): Promise<DetectedMeetingImportProvider | null> {
  if (!ANARLOG_PROVIDER) return null;
  try {
    const found = await hasKnownImportSource();
    if (!found) return null;
    return { ...ANARLOG_PROVIDER, installedAppId: ANARLOG_PROVIDER.id };
  } catch {
    return null;
  }
}

export async function detectImportSources() {
  const [installedResult, anarlogSource] = await Promise.all([
    detectCommands.listInstalledApplications(),
    detectAnarlogFolderSource(),
  ]);
  if (installedResult.status === "error") {
    throw new Error(installedResult.error);
  }

  const providers = [
    ...(anarlogSource ? [anarlogSource] : []),
    ...detectMeetingImportProviders(installedResult.data),
  ];
  if (providers.length === 0) return providers;

  try {
    const iconsResult = await detectCommands.getInstalledApplicationIcons(
      providers.map((provider) => provider.installedAppId),
    );
    if (iconsResult.status === "error") return providers;

    const icons = new Map(
      iconsResult.data.map((icon) => [icon.id, icon.dataUrl]),
    );
    return providers.map((provider) => ({
      ...provider,
      iconUrl: icons.get(provider.installedAppId),
    }));
  } catch {
    return providers;
  }
}
