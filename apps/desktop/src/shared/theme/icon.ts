import { resolveIsDarkMode, type ThemePreference } from "./resolve";

// Was der Nutzer waehlen kann. Bis zum 02.09.2026 standen hier neun geerbte
// Icons (anagram, journal, notepad, stone, typewriter-key, walnut plus die
// Kanal-Icons dev/staging); alle zeigten fremde oder nicht freigegebene
// Motive. Geblieben ist die Welle -- "default" folgt dem Bau-Kanal, "stable"
// erzwingt das Production-Icon (nur in Staging-/Dev-Bauten ein Unterschied,
// und auch dort nur dem Namen nach: alle drei Kanal-Dateien tragen dieselbe
// Welle). ZICK-278.
export type AppIconPreference = "default" | "stable";

// Die Dateien, die das Bundle mitbringt: icons/<name>/icon.icns, in der
// Auslieferung icons/<name>.icns (tauri.conf.json bundle.resources).
export type AppIconName = "stable" | "dev" | "staging";

// Icons mit eigener Dunkelmodus-Fassung (<name>-dark.icns). Leer: die Welle
// ist in beiden Erscheinungsbildern dieselbe Datei, und eine dunkle Fassung
// braucht eine Freigabe. Der Mechanismus bleibt fuer den Tag, an dem sie
// kommt.
const DARK_VARIANT_ICONS: ReadonlySet<AppIconName> = new Set();

// Eine gespeicherte Wahl eines entfernten Icons landet hier auf "default"
// und damit auf der Welle -- kein fehlendes Bild, kein leerer Waehler
// (Rueckweg-Muster wie bei den Argmax-Modellen, 288576e7a0).
export function normalizeAppIconPreference(
  value: string | null | undefined,
): AppIconPreference {
  return value === "stable" ? "stable" : "default";
}

export function resolveAppIconName(
  icon: AppIconPreference,
  appIdentifier: string,
): AppIconName {
  if (icon !== "default") {
    return icon;
  }
  if (appIdentifier.endsWith(".dev")) {
    return "dev";
  }
  if (appIdentifier.endsWith(".staging")) {
    return "staging";
  }
  return "stable";
}

/** `systemIsDark` is the Dock's appearance, used when the theme follows the system. */
export function resolveDockIconName(
  icon: AppIconPreference,
  theme: ThemePreference,
  systemIsDark: boolean,
  appIdentifier: string,
): string {
  const name = resolveAppIconName(icon, appIdentifier);
  return hasDarkAppIconVariant(name) && resolveIsDarkMode(theme, systemIsDark)
    ? `${name}-dark`
    : name;
}

export function hasDarkAppIconVariant(name: AppIconName): boolean {
  return DARK_VARIANT_ICONS.has(name);
}
