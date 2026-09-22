#!/usr/bin/env node
// Sammelt die Lizenzangaben aller fremden Bestandteile und schreibt sie als
// JSON neben den Ueber-Bereich der App. Der Ueber-Bereich importiert diese
// Datei zur Bauzeit, es wird also nichts zur Laufzeit nachgeladen.
//
// WARUM ERZEUGT UND NICHT VON HAND: gemessen am 03.09.2026 haengen an der App
// 1253 fremde Rust-Pakete und 748 npm-Pakete. Eine handgepflegte Liste waere
// nach der naechsten Abhaengigkeit falsch, und eine falsche Namensnennung ist
// schlimmer als eine erzeugte.
//
// OHNE FREMDE DIENSTE: beide Quellen liegen lokal. `cargo metadata --offline`
// liest die Manifeste aus dem Registry-Cache, `pnpm licenses list` liest
// node_modules. Kein Netz, kein Konto, kein `cargo about`.
//
// WARUM --filter-platform aarch64-apple-darwin: ohne den Filter versucht cargo
// auch die Linux-Abhaengigkeiten aufzuloesen (gemessen 03.09.2026: bricht bei
// `alsa v0.11.0` mit "attempting to make an HTTP request, but --offline was
// specified" ab). Der Filter ist ausserdem sachlich richtig -- was auf einem
// Mac nicht gebaut wird, wird auch nicht mitgeliefert und braucht keine
// Namensnennung in dieser App.
//
// Aufruf: node scripts/lizenzen-sammeln.mjs [--pruefen]
//   ohne Schalter  schreibt die JSON-Datei neu
//   --pruefen      schreibt nichts, sondern meldet mit Exit 1, wenn die
//                  abgelegte Datei nicht mehr zum jetzigen Stand passt

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HIER = dirname(fileURLToPath(import.meta.url));
const WURZEL = resolve(HIER, "..");
const ZIEL = resolve(WURZEL, "apps/desktop/src/about/dritte-lizenzen.json");
const PLATTFORM = "aarch64-apple-darwin";

// Lizenzen, die mehr verlangen als "Namen nennen". Wer eine davon findet, muss
// wissen, dass sie da ist -- deshalb stehen sie im Ueber-Bereich einzeln, mit
// Paketnamen, statt in der Gesamtzahl unterzugehen.
const BESONDERS = /LGPL|GPL|MPL|CDLA|BSL|EUPL|SSPL|AFL|CC-BY/i;

function rustPakete() {
  const roh = execFileSync(
    "cargo",
    [
      "metadata",
      "--format-version",
      "1",
      "--offline",
      "--filter-platform",
      PLATTFORM,
      "--manifest-path",
      resolve(WURZEL, "Cargo.toml"),
    ],
    { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  const daten = JSON.parse(roh);
  const eigene = new Set(daten.workspace_members);
  return daten.packages
    .filter((p) => !eigene.has(p.id))
    .map((p) => ({
      name: p.name,
      version: p.version,
      lizenz: p.license ?? null,
      herkunft: p.repository ?? null,
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

function npmPakete() {
  // WARUM cwd statt `pnpm --dir`: mit `--dir` startet corepack im
  // Aufrufverzeichnis, zieht dort seine eigene pnpm-Fassung und bricht dann an
  // der `packageManager`-Angabe des Projekts ab (gemessen 03.09.2026:
  // "This project is configured to use 11.1.1 of pnpm. Your current pnpm is
  // v11.21.0"). Mit gesetztem Arbeitsverzeichnis schaltet corepack von selbst
  // auf die richtige Fassung.
  const roh = execFileSync("pnpm", ["licenses", "list", "--json", "--prod"], {
    cwd: WURZEL,
    encoding: "utf8",
    maxBuffer: 256 * 1024 * 1024,
  });
  const daten = JSON.parse(roh);
  const raus = [];
  for (const [lizenz, liste] of Object.entries(daten)) {
    for (const p of liste) {
      raus.push({
        name: p.name,
        version: Array.isArray(p.versions) ? p.versions.join(", ") : "",
        lizenz: lizenz === "Unknown" ? null : lizenz,
        herkunft: p.homepage ?? null,
      });
    }
  }
  return raus.sort((a, b) => a.name.localeCompare(b.name));
}

function zaehleNachLizenz(pakete) {
  const zaehler = new Map();
  for (const p of pakete) {
    const schluessel = p.lizenz ?? "(keine Angabe)";
    zaehler.set(schluessel, (zaehler.get(schluessel) ?? 0) + 1);
  }
  return [...zaehler.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([lizenz, anzahl]) => ({ lizenz, anzahl }));
}

function bauen() {
  const rust = rustPakete();
  const npm = npmPakete();
  const alle = [...rust, ...npm];
  return {
    // Absichtlich KEIN Zeitstempel: er wuerde bei jedem Lauf einen Diff
    // erzeugen, auch wenn sich keine einzige Abhaengigkeit geaendert hat, und
    // damit --pruefen wertlos machen.
    plattform: PLATTFORM,
    rust: {
      anzahl: rust.length,
      nachLizenz: zaehleNachLizenz(rust),
      pakete: rust,
    },
    npm: { anzahl: npm.length, nachLizenz: zaehleNachLizenz(npm), pakete: npm },
    besondere: alle
      .filter((p) => p.lizenz && BESONDERS.test(p.lizenz))
      .sort((a, b) => a.name.localeCompare(b.name)),
    ohneAngabe: alle
      .filter((p) => !p.lizenz)
      .sort((a, b) => a.name.localeCompare(b.name)),
  };
}

const pruefen = process.argv.includes("--pruefen");
const neu = JSON.stringify(bauen(), null, 2) + "\n";

if (pruefen) {
  if (!existsSync(ZIEL)) {
    console.error(`Fehlt: ${ZIEL}\nEinmal ohne --pruefen laufen lassen.`);
    process.exit(1);
  }
  if (readFileSync(ZIEL, "utf8") !== neu) {
    console.error(
      `Die abgelegte Lizenzliste passt nicht mehr zum jetzigen Stand.\n` +
        `Neu erzeugen:  node scripts/lizenzen-sammeln.mjs`,
    );
    process.exit(1);
  }
  console.log("Lizenzliste ist aktuell.");
} else {
  writeFileSync(ZIEL, neu);
  const daten = JSON.parse(neu);
  console.log(
    `Geschrieben: ${ZIEL}\n` +
      `  Rust: ${daten.rust.anzahl} fremde Pakete\n` +
      `  npm:  ${daten.npm.anzahl} fremde Pakete\n` +
      `  mit besonderen Bedingungen: ${daten.besondere.length}\n` +
      `  ohne Lizenzangabe: ${daten.ohneAngabe.length}`,
  );
}
