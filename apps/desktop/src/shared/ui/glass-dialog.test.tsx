import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  Dialog,
  DialogDescription,
  DialogTitle,
} from "@anlg/ui/components/ui/dialog";

import { GlassDialogContent } from "./glass-dialog";

/**
 * Der tragende Test der Verbindlichkeit.
 *
 * Er sitzt am Bauteil und nicht am Nachfrage-Dialog, weil hier beide
 * Richtungen pruefbar sind: mit `verbindlich` darf Escape und ein Klick
 * daneben nichts ausloesen, ohne `verbindlich` muessen sie es weiterhin tun.
 * Genau diese zweite Haelfte ist die Antwort auf die Frage, ob die Aenderung
 * die anderen Verwender des Bauteils trifft -- sie tut es nicht, und das ist
 * gemessen statt behauptet.
 *
 * Wann er rot wird: sobald der Riegel fehlt (erste Haelfte) oder sobald er
 * ungefragt fuer alle gilt (zweite Haelfte). Was er durchlaesst: ob der
 * Nachfrage-Dialog den Schalter auch wirklich setzt -- das prueft
 * `nachfrage/dialog.test.tsx`.
 */
function bauen(verbindlich?: boolean) {
  const onOpenChange = vi.fn();
  render(
    <Dialog open onOpenChange={onOpenChange}>
      <GlassDialogContent verbindlich={verbindlich}>
        <DialogTitle>Frage</DialogTitle>
        <DialogDescription>Beschreibung</DialogDescription>
        <button type="button" data-testid="drin">
          drin
        </button>
      </GlassDialogContent>
    </Dialog>,
  );
  return { onOpenChange };
}

function escape() {
  fireEvent.keyDown(document.activeElement ?? document.body, {
    key: "Escape",
  });
}

/**
 * Radix hoert auf `pointerdown` ausserhalb des Inhalts. Die Ueberlagerung
 * liegt im Portal neben dem Inhalt und ist die Flaeche, die "daneben" meint.
 *
 * Das Warten ist kein Schmuck: Radix haengt den Zuhoerer erst in einem
 * `setTimeout(0)` an das Dokument. Ohne den Tick lauft der Klick ins Leere und
 * der Test waere gruen, egal was der Code tut.
 */
async function danebenKlicken() {
  await new Promise((weiter) => setTimeout(weiter, 0));
  const ueberlagerung = document.querySelector("[data-dialog-overlay]");
  expect(ueberlagerung).not.toBeNull();
  fireEvent.pointerDown(ueberlagerung!, {
    button: 0,
    ctrlKey: false,
    pointerType: "mouse",
  });
}

describe("GlassDialogContent", () => {
  afterEach(cleanup);

  // Die Grundeinstellung ist "schliessbar". Ohne diesen Test bliebe ein
  // umgedrehter Vorgabewert unbemerkt -- die beiden anderen Verwender waeren
  // still verbindlich geworden, obwohl niemand das wollte.
  it("ist ohne den Schalter schliessbar", () => {
    const { onOpenChange } = bauen();

    escape();

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("laesst Escape schliessen, solange er nicht verbindlich ist", () => {
    const { onOpenChange } = bauen(false);

    escape();

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("laesst einen Klick daneben schliessen, solange er nicht verbindlich ist", async () => {
    const { onOpenChange } = bauen(false);

    await danebenKlicken();

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("schliesst verbindlich nicht bei Escape", () => {
    const { onOpenChange } = bauen(true);

    escape();

    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByTestId("drin")).toBeTruthy();
  });

  it("schliesst verbindlich nicht bei einem Klick daneben", async () => {
    const { onOpenChange } = bauen(true);

    await danebenKlicken();

    expect(onOpenChange).not.toHaveBeenCalled();
    expect(screen.getByTestId("drin")).toBeTruthy();
  });
});
