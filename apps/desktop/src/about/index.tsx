import { Trans, useLingui } from "@lingui/react/macro";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";

import { commands as openerCommands } from "@anlg/plugin-opener2";

import {
  BIBLIOTHEKEN,
  HERKUNFT,
  MODELLE_GELADEN,
  MODELLE_MITGELIEFERT,
  SCHRIFTEN,
  type Nachweis,
} from "./nachweise";

import { SettingsPageTitle } from "~/settings/page-title";

type ErzeugteListe = typeof import("./dritte-lizenzen.json");

function Link({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <button
      type="button"
      className="text-muted-foreground hover:text-foreground underline underline-offset-2"
      onClick={() => void openerCommands.openUrl(href, null)}
    >
      {children}
    </button>
  );
}

function NachweisTabelle({ eintraege }: { eintraege: Nachweis[] }) {
  return (
    <div className="border-border divide-border divide-y rounded-lg border">
      {eintraege.map((e) => (
        <div
          key={e.name}
          className="flex flex-col gap-0.5 px-3 py-2 text-sm sm:flex-row sm:items-baseline sm:justify-between sm:gap-4"
        >
          <div className="min-w-0">
            <div className="font-medium">
              {e.url ? <Link href={e.url}>{e.name}</Link> : e.name}
            </div>
            <div className="text-muted-foreground text-xs">{e.zweck}</div>
          </div>
          <div className="text-muted-foreground shrink-0 text-xs">
            {e.lizenz}
          </div>
        </div>
      ))}
    </div>
  );
}

function Abschnitt({
  titel,
  hinweis,
  children,
}: {
  titel: React.ReactNode;
  hinweis?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="text-base font-semibold">{titel}</h3>
      {hinweis ? (
        <p className="text-muted-foreground text-sm">{hinweis}</p>
      ) : null}
      {children}
    </section>
  );
}

/**
 * Die erzeugte Paketliste wird erst beim Aufklappen geladen. Sie ist rund
 * 350 KB gross (2001 Pakete) und hat im Startbuendel nichts zu suchen.
 */
function PaketListe() {
  const { t } = useLingui();
  const [daten, setDaten] = useState<ErzeugteListe | null>(null);
  const [laedt, setLaedt] = useState(false);
  const [fehler, setFehler] = useState<string | null>(null);

  if (daten) {
    return (
      <div className="flex flex-col gap-4">
        {daten.besondere.length > 0 ? (
          <div className="flex flex-col gap-2">
            <h4 className="text-sm font-medium">
              <Trans>Components with conditions beyond attribution</Trans>
            </h4>
            <div className="border-border divide-border divide-y rounded-lg border text-sm">
              {daten.besondere.map((p) => (
                <div
                  key={`${p.name}@${p.version}`}
                  className="flex items-baseline justify-between gap-4 px-3 py-1.5"
                >
                  <span className="min-w-0 truncate">
                    {p.name} {p.version}
                  </span>
                  <span className="text-muted-foreground shrink-0 text-xs">
                    {p.lizenz}
                  </span>
                </div>
              ))}
            </div>
          </div>
        ) : null}

        {daten.ohneAngabe.length > 0 ? (
          <div className="flex flex-col gap-2">
            <h4 className="text-sm font-medium">
              <Trans>Components without a stated license</Trans>
            </h4>
            <div className="border-border divide-border divide-y rounded-lg border text-sm">
              {daten.ohneAngabe.map((p) => (
                <div
                  key={`${p.name}@${p.version}`}
                  className="truncate px-3 py-1.5"
                >
                  {p.name} {p.version}
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="flex flex-col gap-2">
          <h4 className="text-sm font-medium">
            <Trans>All components</Trans>
          </h4>
          <div className="border-border divide-border max-h-96 divide-y overflow-y-auto rounded-lg border text-sm">
            {[...daten.rust.pakete, ...daten.npm.pakete].map((p, i) => (
              <div
                key={`${p.name}@${p.version}@${i}`}
                className="flex items-baseline justify-between gap-4 px-3 py-1"
              >
                <span className="min-w-0 truncate">
                  {p.name} {p.version}
                </span>
                <span className="text-muted-foreground shrink-0 text-xs">
                  {p.lizenz ?? t`no license stated`}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col items-start gap-2">
      {fehler ? <p className="text-destructive text-sm">{fehler}</p> : null}
      <button
        type="button"
        disabled={laedt}
        className="border-border hover:bg-accent rounded-lg border px-3 py-1.5 text-sm disabled:opacity-60"
        onClick={() => {
          setLaedt(true);
          setFehler(null);
          import("./dritte-lizenzen.json")
            .then((m) => setDaten((m.default ?? m) as ErzeugteListe))
            .catch(() => setFehler(t`The component list could not be loaded.`))
            .finally(() => setLaedt(false));
        }}
      >
        {laedt ? <Trans>Loading…</Trans> : <Trans>Show all components</Trans>}
      </button>
    </div>
  );
}

export function SettingsAbout() {
  const { t } = useLingui();
  // Betreiber, 11.09.2026: „wir brauchen bitte mal eine versionsnummer und diese
  // geht stringend hoch". Drei Baeume hintereinander hiessen 0.1.0, und damit
  // war am Bildschirm nicht zu sehen, welcher Stand gerade laeuft.
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let abgemeldet = false;
    void getVersion()
      .then((v) => {
        if (!abgemeldet) setVersion(v);
      })
      .catch(() => {
        // Eine fehlende Versionsnummer ist kein Fehler, der den Bildschirm
        // stoeren darf -- sie bleibt dann schlicht weg.
      });
    return () => {
      abgemeldet = true;
    };
  }, []);

  return (
    <div className="flex flex-col gap-8">
      <SettingsPageTitle title={t`About`} />

      {version ? (
        <p className="text-muted-foreground -mt-6 text-sm">
          <Trans>Version {version}</Trans>
        </p>
      ) : null}

      <Abschnitt
        titel={<Trans>What this app is built on</Trans>}
        hinweis={
          <Trans>
            Mitschnitt is a fork of {HERKUNFT.projekt} by{" "}
            {HERKUNFT.rechteinhaber}, licensed under {HERKUNFT.lizenz}. It is an
            independent project and is not affiliated with, endorsed by, or
            reviewed by them.
          </Trans>
        }
      >
        <div className="text-sm">
          <Link href={HERKUNFT.url}>{HERKUNFT.url}</Link>
        </div>
      </Abschnitt>

      <Abschnitt
        titel={<Trans>Models included in the app</Trans>}
        hinweis={
          <Trans>
            These are compiled into the program and are part of every copy.
          </Trans>
        }
      >
        <NachweisTabelle eintraege={MODELLE_MITGELIEFERT} />
      </Abschnitt>

      <Abschnitt
        titel={<Trans>Models downloaded on demand</Trans>}
        hinweis={
          <Trans>
            These are not part of the app. They are downloaded when you select
            them and then live in your user folder. Their terms still apply to
            you.
          </Trans>
        }
      >
        <NachweisTabelle eintraege={MODELLE_GELADEN} />
      </Abschnitt>

      <Abschnitt titel={<Trans>Typefaces</Trans>}>
        <NachweisTabelle eintraege={SCHRIFTEN} />
      </Abschnitt>

      <Abschnitt
        titel={<Trans>Replaceable libraries</Trans>}
        hinweis={
          <Trans>
            These ship as separate files inside the app, under
            Contents/Frameworks. Their licence requires that you can replace
            them with your own build. How to do that is described in
            licenses/LIESMICH-libmp3lame.md next to the app.
          </Trans>
        }
      >
        <NachweisTabelle eintraege={BIBLIOTHEKEN} />
      </Abschnitt>

      <Abschnitt
        titel={<Trans>Software components</Trans>}
        hinweis={
          <Trans>
            The app builds on open source work by many other people. The full
            list is generated from the actual dependencies, so it cannot fall
            out of date. The complete terms, including the open points, are in
            ATTRIBUTIONS.md next to the app.
          </Trans>
        }
      >
        <PaketListe />
      </Abschnitt>
    </div>
  );
}
