import { Trans, useLingui } from "@lingui/react/macro";
import { X } from "@phosphor-icons/react";
import { type ComponentRef, useEffect, useRef, useState } from "react";

import { Button } from "@anlg/ui/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";
import { Input } from "@anlg/ui/components/ui/input";

import { titelVorschlag, type Vorschlag } from "./entscheidung";

import { ParticipantsDisplay } from "~/session/components/outer-header/metadata/participants";
import {
  GlassDialogCancelButton,
  GlassDialogContent,
} from "~/shared/ui/glass-dialog";

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
   * Ueberspringen -- auf jedem Weg, der aus dem Dialog fuehrt, ausser dem
   * Bestaetigen-Knopf: dem Skip-Knopf, dem Schliessen-X, Escape und einem
   * Klick daneben. Bis zum 26.09.2026 war der Dialog `verbindlich` und all
   * diese Wege waren gesperrt -- ein Gurt ohne sichtbaren Gurtknopf, den
   * der Nutzer nur ueber die Bestaetigung wieder loswurde. Jetzt ist es ein
   * echter Ausgang: keine Teilnehmer werden neu angelegt, ein bereits
   * vorhandener Titel bleibt unangetastet (`nachfrageSpeichern` schreibt ihn
   * hier ohnehin nicht, siehe `host.tsx`), und die wartende Zusammenfassung
   * laeuft an. `host.tsx` markiert die Sitzung dabei zusaetzlich als
   * beantwortet (`gate.ts` `markiereNachfrageBeantwortet`) -- ein zweiter
   * Stopp derselben Sitzung fragt seit 27.09.2026 nicht erneut. Bleibt
   * daneben verdrahtet fuer das Schliessen aus einer anderen Richtung
   * (Fenster weg, Sitzung verschwindet).
   */
  onVerwerfen: () => void;
}) {
  const { t } = useLingui();
  const [titel, setTitel] = useState("");
  const [titelBeruehrt, setTitelBeruehrt] = useState(false);
  // Fuer die Escape-Weiche: der Inhalt DIESES Dialogs, nicht "irgendwo im
  // Dokument". Siehe deren Kommentar unten.
  const inhaltRef = useRef<ComponentRef<typeof GlassDialogContent>>(null);

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
      <GlassDialogContent
        ref={inhaltRef}
        // Zwei Faelle, in denen dieser Escape NICHT ueberspringen darf:
        //
        // 1. Waehrend des Speicherns bleibt der Dialog verbindlich: der
        //    Confirm-Klick hat bereits eine Antwort abgegeben, und ein Escape
        //    in genau diesem kurzen Fenster duerfte die Zusammenfassung nicht
        //    VOR dem Titel-Schreiben freigeben. Ausserhalb von `laeuft` ist
        //    das keine Sperre mehr, siehe `onVerwerfen` oben.
        //
        // 2. Die Vorschlagsliste des Teilnehmer-Blocks ist offen: dann
        //    gehoert dieser Escape IHR (sie schliesst sich selbst, siehe
        //    `input.tsx` `resetInput()`), nicht dem Dialog. Das laesst sich
        //    nicht von innen abfangen (`stopPropagation` in der Liste kaeme
        //    zu spaet): Radix' `useEscapeKeydown` haengt seinen Zuhoerer mit
        //    `{ capture: true }` an `document` und ruft genau DIESES
        //    `onEscapeKeyDown` synchron auf, BEVOR die Liste ueberhaupt in
        //    ihrer eigenen Bubble-Phase ankommt (`@radix-ui/react-dismissable-
        //    layer`: `onEscapeKeyDown?.(event); if (!event.defaultPrevented
        //    && onDismiss) onDismiss();`). Der einzige Ort, an dem
        //    `preventDefault()` noch etwas bewirkt, ist deshalb hier.
        //
        //    Die Abfrage ist bewusst auf `inhaltRef.current` gescopt, NICHT
        //    auf das ganze Dokument: ein `document.querySelector` haette auch
        //    eine voellig fremde, anderswo offene Vorschlagsliste getroffen
        //    (z. B. im Kopfzeilen-Popover einer anderen Sitzung) und Escape
        //    hier faelschlich blockiert. Das Merkmal
        //    `data-participant-suggestions-open` sitzt deshalb an der
        //    Eingabezeile (`input.tsx`), die -- anders als die Liste selbst --
        //    ein echter DOM-Nachkomme dieses Dialogs bleibt; die Liste ist
        //    per FloatingPortal ausgelagert und waere ueber diesen Ref nie
        //    erreichbar.
        onEscapeKeyDown={(event) => {
          if (laeuft) {
            event.preventDefault();
            return;
          }
          if (
            inhaltRef.current?.querySelector(
              "[data-participant-suggestions-open]",
            )
          ) {
            event.preventDefault();
          }
        }}
        // Ein Klick auf einen Vorschlag darf nie als "Klick daneben" zaehlen.
        // Der EIGENTLICHE, notwendige Fix sitzt in `dropdown.tsx`: modale
        // Radix-Dialoge setzen beim Oeffnen
        // `document.body.style.pointerEvents = "none"` und geben es nur dem
        // eigenen Content-Knoten und dem Overlay explizit wieder frei
        // (`@radix-ui/react-dismissable-layer`, `@radix-ui/react-dialog`,
        // installierter Quelltext geprueft). Die Vorschlagsliste haengt per
        // FloatingPortal als eigener Knoten direkt am <body>, ausserhalb
        // dieses Dialog-Inhalts, und erbte deshalb `pointer-events:none` --
        // ein Klick fiel durch sie hindurch auf den (tiefer liegenden)
        // Overlay, der ihn als Klick daneben las und ueberspringen liess,
        // OHNE dass die Auswahl je ankam. Behoben dort per
        // `pointer-events-auto`.
        //
        // Der `closest()`-Check hier ist zusaetzliche, per Rot-Beweis als
        // AKTUELL UEBERFLUESSIG entlarvte Vorsicht, keine notwendige
        // Bedingung: React leitet Ereignisse aus einem `createPortal`-Kind
        // entlang des REACT-Baums weiter, nicht des DOM-Baums (offiziell
        // dokumentiert), und Radix' `onPointerDownCapture` auf dem
        // Content-Knoten (`isPointerInsideReactTreeRef`) erkennt einen Klick
        // in dieser Liste deshalb schon von sich aus als "innen" -- ein
        // Mutant, der genau diesen Check hier abschaltet, laesst den
        // D1-Test unten weiter gruen (getestet). Er bleibt trotzdem stehen:
        // billige, explizite Absicherung fuer den Fall, dass sich dieses
        // undokumentierte Radix-Verhalten mit einem kuenftigen Upgrade
        // aendert.
        onInteractOutside={(event) => {
          if (laeuft) {
            event.preventDefault();
            return;
          }
          const ziel = event.target as Element | null;
          if (ziel?.closest?.("[data-participant-suggestions]")) {
            event.preventDefault();
          }
        }}
      >
        <DialogClose
          disabled={laeuft}
          aria-label={t`Skip`}
          data-testid="nachfrage-schliessen"
          className="text-muted-foreground ring-offset-background focus:ring-ring absolute top-4 right-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none disabled:opacity-40"
        >
          <X className="h-4 w-4" />
        </DialogClose>
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

        <DialogFooter className="grid grid-cols-2 gap-2 sm:grid-cols-2 sm:justify-normal">
          <GlassDialogCancelButton
            disabled={laeuft}
            data-testid="nachfrage-ueberspringen"
            onClick={onVerwerfen}
          >
            <Trans>Skip</Trans>
          </GlassDialogCancelButton>
          <Button
            className="bg-primary text-primary-foreground hover:bg-primary/90 h-8 rounded-full px-4 text-xs font-medium shadow-sm dark:bg-white dark:text-black dark:hover:bg-white/90"
            disabled={laeuft}
            data-testid="nachfrage-bestaetigen"
            onClick={() => onUebernehmen({ titel: angezeigterTitel })}
          >
            {nurIch ? (
              <Trans>Only me, continue</Trans>
            ) : (
              <Trans>Continue</Trans>
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
