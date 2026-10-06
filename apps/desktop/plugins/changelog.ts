import { readFileSync, watch } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { Plugin, ViteDevServer } from "vite";

// Eine Quelle: CHANGELOG.md im Repo-Root. Bis zum 29.09.2026 las das Plugin
// packages/changelog/content/<version>.md; der Ordner ist seit 01.09. leer
// (dort lag die fremde Upstream-Geschichte), deshalb zeigte der Dialog
// "Was ist neu" nur noch "kein Aenderungsprotokoll". CHANGELOG.md ist
// ohnehin Pflicht vor jedem Release (scripts/mitschnitt-release.sh bricht
// ohne Abschnitt ab) und speist schon GitHub-Notiz und Updater-Dialog.
//
// Pfade erst beim Aufruf berechnen, nicht beim Import: der Test importiert nur
// die reine Umwandlung, und unter jsdom ist import.meta.url keine file:-URL
// ("The URL must be of scheme file", gemessen 29.09.2026).
function pluginDir(): string {
  return fileURLToPath(new URL(".", import.meta.url));
}
function changelogFile(): string {
  return resolve(pluginDir(), "../../../CHANGELOG.md");
}
function packageJsonFile(): string {
  return resolve(pluginDir(), "../package.json");
}

const VIRTUAL_ID = "virtual:changelog";
const RESOLVED_ID = "\0" + VIRTUAL_ID;

// Die App-Version kommt aus apps/desktop/package.json, nicht aus einer
// Umgebungsvariable: nur so stimmt sie mit dem ueberein, was der Nutzer im
// Dialog als "Was ist neu in X?" sieht.
function getAppVersion(): string | null {
  try {
    const raw = readFileSync(packageJsonFile(), "utf-8");
    const parsed = JSON.parse(raw) as { version?: unknown };
    if (typeof parsed.version === "string" && parsed.version.length > 0) {
      return parsed.version;
    }
    return null;
  } catch {
    return null;
  }
}

// Fuehrende und folgende Leerzeilen entfernen -- dieselbe Wirkung wie die
// sed-Idiome in scripts/changelog-extract.sh, damit ein Abschnitt mit
// Leerzeile direkt nach der Ueberschrift nicht als leer gilt.
function trimBlankLines(text: string): string {
  const lines = text.split("\n");
  let start = 0;
  let end = lines.length;
  while (start < end && lines[start].trim() === "") start++;
  while (end > start && lines[end - 1].trim() === "") end--;
  return lines.slice(start, end).join("\n");
}

// Markdown fuer die einzeilige summary entfernen: Links [Text](URL) -> Text,
// dann **fett**/__fett__ und danach das verbleibende einzelne *kursiv*.
// Reihenfolge wie in scripts/changelog-extract.sh (notes-plain), damit
// GitHub-Notiz und In-App-Anzeige denselben Text zeigen.
function stripMarkdown(text: string): string {
  return text
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/__([^_]+)__/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .trim();
}

// Erster Stichpunkt von "What's new" als eine Zeile, ohne Markdown. Die
// Anzeige (packages/changelog/src/process.ts) liest summary aus dem
// Frontmatter und erwartet dort einen einfachen Wert.
function firstBulletAsSummary(whatsNew: string): string {
  const lines = whatsNew.split("\n");
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    // Stichpunkte beginnen mit "- " oder "* "; falls der Abschnitt mit einer
    // Zwischenueberschrift oder Fettzeile beginnt, nehmen wir diese Zeile.
    const bullet = trimmed.replace(/^[-*]\s+/, "");
    const plain = stripMarkdown(bullet);
    if (plain !== "") return plain;
  }
  return "";
}

// Doppelte Anfuehrungszeichen im Frontmatter-Wert escapen, sonst bricht der
// Parser in process.ts am ersten inneren " ab.
function escapeDoubleQuotes(text: string): string {
  return text.replace(/"/g, '\\"');
}

// Reine Funktion ohne fs-Zugriff, damit sie in vitest direkt pruefbar ist.
// Regeln genau wie scripts/changelog-extract.sh:
//   - Abschnitt beginnt bei der Zeile, die exakt mit "## <version> (" beginnt
//     (Zeichenkettenvergleich, keine Regex ueber die Version -- Punkte!),
//     endet vor der naechsten Zeile, die mit "## " beginnt.
//   - Datum aus der Klammer (YYYY-MM-DD) der Kopfzeile.
//   - "### What's new" ist Pflicht und nicht leer, sonst null.
//   - "### Note" optional.
export function changelogSectionToAppMarkdown(
  changelogMd: string,
  version: string,
): string | null {
  const lines = changelogMd.split("\n");

  // Schritt 1: vollen Koerper des Versions-Abschnitts isolieren.
  let sectionStart = -1;
  let sectionEnd = -1;
  let date: string | null = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (!line.startsWith("## ")) continue;

    const rest = line.slice(3);
    const parenIndex = rest.indexOf(" (");
    const headingVersion = parenIndex > 0 ? rest.slice(0, parenIndex) : rest;

    if (sectionStart >= 0) {
      // Naechste "## "-Ueberschrift beendet den gesuchten Abschnitt.
      sectionEnd = i;
      break;
    }

    if (headingVersion === version) {
      sectionStart = i + 1;
      // Datum aus der Klammer der Kopfzeile ziehen.
      const dateMatch = rest.match(/\((\d{4}-\d{2}-\d{2})\)/);
      date = dateMatch ? dateMatch[1] : null;
    }
  }

  if (sectionStart < 0) return null;
  if (sectionEnd < 0) sectionEnd = lines.length;

  const sectionLines = lines.slice(sectionStart, sectionEnd);

  // Schritt 2: Unterabschnitte "### <ueberschrift>" herausziehen.
  function subSection(heading: string): string {
    let active = false;
    const collected: string[] = [];
    for (const line of sectionLines) {
      if (line.startsWith("### ")) {
        active = line.slice(4) === heading;
        continue;
      }
      if (active) collected.push(line);
    }
    return trimBlankLines(collected.join("\n"));
  }

  const whatsNew = subSection("What's new");
  if (whatsNew === "") return null;

  const note = subSection("Note");

  const summary = firstBulletAsSummary(whatsNew);

  const parts: string[] = [];
  parts.push("---");
  parts.push(`date: "${date ?? ""}"`);
  parts.push(`summary: "${escapeDoubleQuotes(summary)}"`);
  parts.push("---");
  parts.push("");

  if (note !== "") {
    parts.push(`<banner title="Note" variant="info">`);
    parts.push(note);
    parts.push(`</banner>`);
    parts.push("");
  }

  parts.push(whatsNew);

  return parts.join("\n");
}

function buildModule(): string {
  const version = getAppVersion();
  let latest: string | null = null;
  let content: string | null = null;

  if (version) {
    try {
      const changelogMd = readFileSync(changelogFile(), "utf-8");
      const converted = changelogSectionToAppMarkdown(changelogMd, version);
      if (converted !== null) {
        latest = version;
        content = converted;
      }
    } catch {
      // Datei fehlt oder ist unlesbar: dann bleibt es beim "No changelog"-
      // Pfad in data.ts, statt den Bau mit einem Fehler abzubrechen.
    }
  }

  return [
    `export const latestVersion = ${JSON.stringify(latest)};`,
    `export const latestContent = ${JSON.stringify(content)};`,
  ].join("\n");
}

export function changelog(): Plugin {
  return {
    name: "changelog",
    resolveId(id) {
      if (id === VIRTUAL_ID) return RESOLVED_ID;
    },
    load(id) {
      if (id === RESOLVED_ID) return buildModule();
    },
    configureServer(server: ViteDevServer) {
      if (process.env.NODE_ENV === "test" || process.env.VITEST) {
        return;
      }

      // Im Dev-Betrieb soll ein geaenderter Changelog sofort im Dialog
      // erscheinen, ohne Neustart des Vite-Servers.
      try {
        watch(changelogFile(), () => {
          const mod = server.moduleGraph.getModuleById(RESOLVED_ID);
          if (mod) {
            server.moduleGraph.invalidateModule(mod);
            server.ws.send({ type: "full-reload" });
          }
        });
      } catch {}
    },
  };
}
