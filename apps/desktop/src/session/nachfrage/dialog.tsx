import { Trans, useLingui } from "@lingui/react/macro";
import { useEffect, useState } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import {
  Dialog,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";
import { Input } from "@anlg/ui/components/ui/input";

import { titelVorschlag, type Vorschlag } from "./entscheidung";

import { ParticipantsDisplay } from "~/session/components/outer-header/metadata/participants";
import { GlassDialogContent } from "~/shared/ui/glass-dialog";

/**
 * Die Frage, die einmal vor der Zusammenfassung kommt: wer war dabei, und wie
 * heisst das Gespraech.
 *
 * Die Teilnehmer-Haelfte ist NICHT selbst gebaut. Sie ist woertlich der Block
 * aus dem Metadaten-Popover oben rechts in der Sitzung
 * (`outer-header/metadata/participants`) -- Merkzettel mit x zum Entfernen,
 * "Add participants" als Einstieg, und die Vorschlaege aus `humans` beim
 * Tippen. Damit gilt hier dieselbe Regel wie ueberall in der App: ein Mensch
 * ist eine Entitaet, Zuordnen heisst auswaehlen statt tippen.
 *
 * Folge davon, ausdruecklich: dieser Block schreibt seine Teilnehmer SOFORT
 * in die Datenbank, nicht erst beim Bestaetigen -- genau wie im Popover. Die
 * Antwort dieses Dialogs traegt deshalb nur noch den Titel.
 *
 * Der Rest ist Oberflaeche ohne Store und ohne Queries. Wer sie fuellt und wer
 * die Antwort wegschreibt, steht in `host.tsx`.
 */
export function NachfrageDialog({
  offen,
  sessionId,
  teilnehmerNamen,
  titelVorher,
  zeitpunkt,
  laeuft = false,
  onUebernehmen,
  onVerwerfen,
}: {
  offen: boolean;
  /** Fuer den uebernommenen Teilnehmer-Block, der selbst an der DB haengt. */
  sessionId: string;
  /** Namen der Teilnehmer OHNE den Menschen am Mikrofon, live aus der DB. */
  teilnehmerNamen: string[];
  titelVorher: string;
  zeitpunkt: Date;
  laeuft?: boolean;
  onUebernehmen: (antwort: { titel: string }) => void;
  /**
   * Gurt, nicht Weg. Seit der Dialog `verbindlich` ist, kommen weder Escape
   * noch ein Klick daneben hier an -- der einzige Ausgang ist der Knopf. Bleibt
   * verdrahtet, damit ein Schliessen aus einer anderen Richtung (Fenster weg,
   * Sitzung verschwindet) die wartende Zusammenfassung trotzdem freigibt statt
   * sie haengen zu lassen.
   */
  onVerwerfen: () => void;
}) {
  const { t } = useLingui();
  const [titel, setTitel] = useState("");
  const [titelBeruehrt, setTitelBeruehrt] = useState(false);

  useEffect(() => {
    if (!offen) {
      setTitel("");
      setTitelBeruehrt(false);
    }
  }, [offen]);

  const vorschlag = titelVorschlag({
    titel: titelVorher,
    teilnehmer: teilnehmerNamen,
  });
  const vorschlagText = useVorschlagText(vorschlag, zeitpunkt);
  // Solange niemand am Feld war, folgt der Vorschlag den eingetragenen Namen.
  // Ab der ersten Taste gehoert das Feld dem Menschen.
  const angezeigterTitel = titelBeruehrt ? titel : vorschlagText;

  // Niemand ausser dem Menschen am Mikrofon: dann ist die Bestaetigung ein
  // Weiter-Knopf und keine Formulararbeit.
  const nurIch = teilnehmerNamen.length === 0;

  return (
    <Dialog
      open={offen}
      onOpenChange={(naechster) => {
        if (!naechster) {
          onVerwerfen();
        }
      }}
    >
      <GlassDialogContent verbindlich>
        <DialogHeader className="items-center gap-2 text-center sm:text-center">
          <DialogTitle className="text-foreground text-[13px] leading-5 font-semibold tracking-normal">
            <Trans>Who was in this meeting?</Trans>
          </DialogTitle>
          <DialogDescription className="text-foreground w-full text-center text-[13px] leading-[1.36]">
            <Trans>Names here are recognized in the transcript.</Trans>
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-2">
          {/* Dieselbe Zeile wie das Datum im Popover: randlos, kein Etikett. */}
          <div className="flex h-7 items-center">
            <Input
              type="text"
              aria-label={t`Title`}
              value={angezeigterTitel}
              disabled={laeuft}
              autoFocus
              className="h-7 flex-1 border-0 px-0 py-0 shadow-none focus-visible:ring-0"
              onChange={(event) => {
                setTitelBeruehrt(true);
                setTitel(event.target.value);
              }}
            />
          </div>

          {/* Bringt seine eigene Trennlinie mit -- wie im Popover. */}
          <ParticipantsDisplay sessionId={sessionId} />
        </div>

        <DialogFooter className="gap-2 sm:justify-end">
          <Button
            className="bg-primary text-primary-foreground hover:bg-primary/90 h-8 rounded-full px-4 text-xs font-medium shadow-sm dark:bg-white dark:text-black dark:hover:bg-white/90"
            disabled={laeuft}
            data-testid="nachfrage-bestaetigen"
            onClick={() => onUebernehmen({ titel: angezeigterTitel })}
          >
            {nurIch ? (
              <Trans>Only me, continue</Trans>
            ) : (
              <Trans>Save and summarize</Trans>
            )}
          </Button>
        </DialogFooter>
      </GlassDialogContent>
    </Dialog>
  );
}

/**
 * Der Vorschlagstext.
 *
 * Muss ein Hook sein und `t` selbst aus `useLingui()` holen: reicht man `t`
 * als Parameter durch, findet das lingui-Makro die Texte beim Extrahieren
 * nicht, sie fehlen in allen Katalogen und erscheinen fuer immer englisch.
 * Genau das ist hier einmal passiert -- gefangen hat es der Katalog-Test,
 * nicht die Oberflaechen-Tests, die `t` ohnehin nachbilden.
 */
function useVorschlagText(vorschlag: Vorschlag, zeitpunkt: Date): string {
  const { t } = useLingui();

  switch (vorschlag.art) {
    case "vorhanden":
      return vorschlag.titel;
    case "mit-einem":
      return t`Conversation with ${vorschlag.name}`;
    case "mit-mehreren":
      return t`Conversation with ${vorschlag.name} and ${vorschlag.weitere} others`;
    case "nur-zeit":
      return t`Conversation on ${zeitpunktText(zeitpunkt)}`;
  }
}

function zeitpunktText(zeitpunkt: Date): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(zeitpunkt);
  } catch {
    return zeitpunkt.toISOString();
  }
}
