import { Trans } from "@lingui/react/macro";
import { ArrowElbowDownLeft } from "@phosphor-icons/react";
import { type CSSProperties, useEffect, useMemo, useRef } from "react";

import { cn } from "@anlg/utils";

type DropdownOption = {
  id: string;
  name: string;
  isNew?: boolean;
  email?: string;
  phone?: string;
  orgId?: string;
  jobTitle?: string;
};

/** Normalisiert einen Anzeigenamen fuer den Dubletten-Vergleich. */
function angezeigterNameNormalisiert(name: string): string {
  return name.trim().toLocaleLowerCase();
}

export function ParticipantDropdown({
  floatingRef,
  floatingStyles,
  options,
  selectedIndex,
  onSelect,
  onHover,
}: {
  floatingRef: (node: HTMLDivElement | null) => void;
  floatingStyles: CSSProperties;
  options: DropdownOption[];
  selectedIndex: number;
  onSelect: (option: DropdownOption) => void;
  onHover: (index: number) => void;
}) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list) return;

    const selectedElement = list.children[selectedIndex] as HTMLElement;
    if (selectedElement) {
      selectedElement.scrollIntoView({ block: "nearest" });
    }
  }, [selectedIndex, options]);

  /**
   * Zwei echte Menschen koennen denselben Anzeigenamen tragen (gemessen im
   * Nachfrage-Dialog 26.09.2026: derselbe Name samt Firmenzusatz zweimal, zwei
   * verschiedene Adressbuch-Eintraege). Ohne Unterscheidung ist die Liste
   * fuer die Maus wie fuer die Augen gleich -- welchen der beiden man trifft,
   * ist Zufall. Gruppiert wird nur unter den echten Kandidaten, nie unter dem
   * angebotenen "Add ...".
   */
  const gruppenNachName = useMemo(() => {
    const gruppen = new Map<string, DropdownOption[]>();
    for (const option of options) {
      if (option.isNew) {
        continue;
      }
      const schluessel = angezeigterNameNormalisiert(option.name);
      const bestehend = gruppen.get(schluessel);
      if (bestehend) {
        bestehend.push(option);
      } else {
        gruppen.set(schluessel, [option]);
      }
    }
    return gruppen;
  }, [options]);

  if (options.length === 0) {
    return null;
  }

  return (
    <div
      ref={floatingRef}
      style={floatingStyles}
      // Merkmal fuer diesen Dialog (`session/nachfrage/dialog.tsx`): ein
      // Klick auf einen Vorschlag darf ihn nicht als "Klick daneben"
      // ueberspringen. `onInteractOutside` prueft den ECHTEN Klick-Zielpfad
      // (`event.target.closest(...)`), deshalb genuegt hier ein Merkmal ohne
      // Bezug auf eine bestimmte Sitzung -- ein Klick, der es traf, war per
      // Definition INNERHALB einer Vorschlagsliste, unabhaengig davon,
      // welcher Dialog/Popover sie gerade zeigt. Fuer "ist GENAU MEINE Liste
      // offen" (Escape-Weiche) trägt stattdessen die nicht-portalierte
      // Eingabezeile ihr eigenes Merkmal, siehe `input.tsx`
      // `data-participant-suggestions-open`.
      data-participant-suggestions
      // z-[60] + pointer-events-auto: liegen bewusst ueber jedem Dialog
      // dieser App. Ein modaler Radix-Dialog setzt beim Oeffnen
      // `document.body.style.pointerEvents = "none"` und gibt es nur seinem
      // EIGENEN Content-Knoten und dem Overlay explizit wieder frei (siehe
      // `@radix-ui/react-dismissable-layer` + `@radix-ui/react-dialog` im
      // installierten Quelltext). Diese Liste haengt als eigener
      // FloatingPortal-Knoten NEBEN dem Dialog direkt am <body> und erbt das
      // `pointer-events:none` -- ohne die eigene Freigabe faellt jeder Klick
      // durch sie hindurch auf den (weiter unten liegenden) Overlay, der ihn
      // als Klick daneben liest und den Dialog ueberspringen laesst, OHNE
      // dass die Auswahl je ankommt. z-[60] allein macht sie nur SICHTBAR,
      // nicht klickbar -- beides gemessen am Nachfrage-Dialog, 26.09.2026.
      className="bg-popover pointer-events-auto z-[60] overflow-hidden rounded-md border shadow-md"
      onMouseDown={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      <div ref={listRef} className="max-h-50 overflow-auto py-1">
        {options.map((option, index) => {
          const gruppe = option.isNew
            ? null
            : gruppenNachName.get(angezeigterNameNormalisiert(option.name));
          const zweideutig = Boolean(gruppe && gruppe.length > 1);
          // Fuer die eigene Unterzeile zaehlt nur Mail/Telefon -- die
          // Position (jobTitle) steht bereits UNBEDINGT inline daneben
          // (weiter unten). Waere sie hier zusaetzlich Kandidat, stuende sie
          // bei einer erfolgreichen Unterscheidung zweimal auf derselben
          // Zeile (gemessen per Test: "Vertrieb" erschien doppelt).
          const unterscheidung =
            zweideutig && gruppe
              ? (eindeutigerWertImVergleich("email", option, gruppe) ??
                eindeutigerWertImVergleich("phone", option, gruppe))
              : null;
          // Steht KEINE Unterzeile an, kann die inline gezeigte Position
          // trotzdem schon reichen -- dann muss nichts mehr unterscheiden.
          const jobTitleUnterscheidetBereitsInline =
            zweideutig &&
            gruppe &&
            eindeutigerWertImVergleich("jobTitle", option, gruppe) !== null;
          // Letzter Ausweg: nichts an diesem Datensatz unterscheidet ihn von
          // den anderen der Gruppe (weder Mailadresse noch Telefonnummer
          // noch Position -- oder sie teilen sich sogar denselben Wert).
          // Eine stabile Nummer nach Listenposition haelt die Auswahl
          // wenigstens benennbar, statt zwei optisch identische Zeilen
          // stehen zu lassen.
          const angezeigterName =
            zweideutig &&
            !unterscheidung &&
            !jobTitleUnterscheidetBereitsInline &&
            gruppe
              ? `${option.name} (${gruppe.indexOf(option) + 1})`
              : option.name;

          return (
            <button
              key={option.id}
              type="button"
              tabIndex={-1}
              className={cn([
                "w-full px-3 py-1.5 text-left text-sm",
                selectedIndex === index ? "bg-muted" : "hover:bg-accent",
              ])}
              onClick={() => onSelect(option)}
              onMouseEnter={() => onHover(index)}
            >
              <span className="flex w-full items-center justify-between">
                {option.isNew ? (
                  <span>
                    <Trans>
                      Add "<span className="font-medium">{option.name}</span>"
                    </Trans>
                  </span>
                ) : (
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="flex items-center gap-2">
                      <span className="font-medium">{angezeigterName}</span>
                      {option.jobTitle && (
                        <span className="text-muted-foreground text-xs">
                          {option.jobTitle}
                        </span>
                      )}
                    </span>
                    {unterscheidung && (
                      <span className="text-muted-foreground truncate text-[11px]">
                        {unterscheidung}
                      </span>
                    )}
                  </span>
                )}
                {selectedIndex === index && (
                  <ArrowElbowDownLeft className="text-muted-foreground size-3 shrink-0" />
                )}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/**
 * Prueft EIN Feld fuer EINEN Eintrag gegen den Rest seiner Namens-Gruppe:
 * leer oder von einem anderen Mitglied geteilt zaehlt nicht als
 * Unterscheidung (zwei Personen mit gleichem Namen und derselben Mailadresse
 * sind damit immer noch nicht auseinanderzuhalten).
 */
function eindeutigerWertImVergleich(
  feld: "email" | "phone" | "jobTitle",
  option: DropdownOption,
  gruppe: DropdownOption[],
): string | null {
  const wert = option[feld]?.trim();
  if (!wert) {
    return null;
  }
  const normalisiert = wert.toLocaleLowerCase();
  const teiltDenWert = gruppe.some(
    (andere) =>
      andere.id !== option.id &&
      (andere[feld]?.trim().toLocaleLowerCase() ?? "") === normalisiert,
  );
  return teiltDenWert ? null : wert;
}
