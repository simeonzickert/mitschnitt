import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

const lingui = vi.hoisted(() => {
  const t = (
    input: TemplateStringsArray | string,
    ...werte: unknown[]
  ): string => {
    if (Array.isArray(input)) {
      return input.reduce(
        (text: string, teil: string, i: number) =>
          `${text}${teil}${i < werte.length ? String(werte[i]) : ""}`,
        "",
      );
    }
    return String(input);
  };
  return { t };
});

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useLingui: () => ({ _: lingui.t, t: lingui.t }),
}));

// Der Teilnehmer-Block ist der echte Block aus dem Metadaten-Popover und
// haengt an Datenbank, Live-Queries und floating-ui. Hier wird nur geprueft,
// dass er ueberhaupt mit der richtigen Sitzung eingesetzt ist -- sein
// Verhalten pruefen seine eigenen Tests, an seinem eigenen Ort.
vi.mock("~/session/components/outer-header/metadata/participants", () => ({
  ParticipantsDisplay: ({ sessionId }: { sessionId: string }) => (
    <div data-testid="teilnehmer-block" data-session={sessionId} />
  ),
}));

import { NachfrageDialog } from "./dialog";

const zeitpunkt = new Date("2026-09-04T09:20:00Z");

function bauen(props: Partial<Parameters<typeof NachfrageDialog>[0]> = {}) {
  const onUebernehmen = vi.fn();
  const onVerwerfen = vi.fn();
  render(
    <NachfrageDialog
      offen
      sessionId="s1"
      teilnehmerNamen={[]}
      titelVorher=""
      zeitpunkt={zeitpunkt}
      onUebernehmen={onUebernehmen}
      onVerwerfen={onVerwerfen}
      {...props}
    />,
  );
  return { onUebernehmen, onVerwerfen };
}

const titelFeld = () => screen.getByLabelText("Title") as HTMLInputElement;

describe("NachfrageDialog", () => {
  afterEach(cleanup);

  // Mangel 4/5/6: die Teilnehmer sind nicht nachgebaut, sondern das Bauteil
  // aus dem Metadaten-Popover -- mit Merkzetteln, x zum Entfernen und den
  // Vorschlaegen aus `humans`.
  it("setzt den Teilnehmer-Block der App fuer diese Sitzung ein", () => {
    bauen({ sessionId: "s42" });

    expect(screen.getByTestId("teilnehmer-block").dataset.session).toBe("s42");
  });

  // "Nur ich" ist ein Knopf, keine Formulararbeit.
  it("bestaetigt ohne weitere Teilnehmer als Nur-ich", () => {
    const { onUebernehmen } = bauen();

    expect(screen.getByText("Only me, continue")).toBeTruthy();
    fireEvent.click(screen.getByTestId("nachfrage-bestaetigen"));

    expect(onUebernehmen).toHaveBeenCalledWith({
      titel: expect.stringContaining("Conversation on "),
    });
  });

  it("nennt den Knopf um, sobald jemand dabei war", () => {
    bauen({ teilnehmerNamen: ["Anna Beispiel"] });

    expect(screen.getByText("Save and summarize")).toBeTruthy();
  });

  // Der Titel wird vorgeschlagen, nicht abgefragt: ein leeres Feld ist eine
  // Aufgabe, ein gefuellter Vorschlag ist eine Bestaetigung.
  it("schlaegt den Zeitpunkt vor, solange nichts vorliegt", () => {
    bauen();
    expect(titelFeld().value).toContain("Conversation on ");
  });

  it("folgt den Teilnehmern, solange niemand am Titelfeld war", () => {
    bauen({ teilnehmerNamen: ["Anna Beispiel"] });

    expect(titelFeld().value).toBe("Conversation with Anna Beispiel");
  });

  it("uebernimmt einen vorhandenen Titel unveraendert", () => {
    bauen({ titelVorher: "Jour fixe", teilnehmerNamen: ["Anna Beispiel"] });
    expect(titelFeld().value).toBe("Jour fixe");
  });

  // Ab der ersten Taste gehoert das Feld dem Menschen.
  it("laesst einen getippten Titel von neuen Namen unangetastet", () => {
    const { onUebernehmen } = bauen();

    fireEvent.change(titelFeld(), { target: { value: "Von Hand" } });
    expect(titelFeld().value).toBe("Von Hand");

    fireEvent.click(screen.getByTestId("nachfrage-bestaetigen"));
    expect(onUebernehmen).toHaveBeenCalledWith({ titel: "Von Hand" });
  });

  // Die Frage endet mit einer Entscheidung, nie mit nichts. Bis zum 04.09.
  // schloss Escape sie lautlos -- unsichtbar fuer den, der raus will, und ein
  // Versehen fuer den, der es nicht wollte. Der Schalter dafuer sitzt am
  // Bauteil (`shared/ui/glass-dialog`); hier wird geprueft, dass der Dialog
  // ihn auch setzt.
  it("schliesst nicht, wenn Escape gedrueckt wird", () => {
    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: "Escape",
    });

    expect(onVerwerfen).not.toHaveBeenCalled();
    expect(onUebernehmen).not.toHaveBeenCalled();
    expect(screen.getByTestId("nachfrage-bestaetigen")).toBeTruthy();
  });

  it("schliesst nicht, wenn daneben geklickt wird", async () => {
    const { onVerwerfen, onUebernehmen } = bauen();

    // Radix haengt seinen Zuhoerer fuer Klicks nach draussen erst in einem
    // `setTimeout(0)` an das Dokument. Ohne diesen Tick liefe der Klick ins
    // Leere und der Test waere gruen, egal was der Code tut -- gemessen: mit
    // ausgebautem Riegel blieb er gruen, bis das Warten drin war.
    await new Promise((weiter) => setTimeout(weiter, 0));
    const ueberlagerung = document.querySelector("[data-dialog-overlay]");
    expect(ueberlagerung).not.toBeNull();
    fireEvent.pointerDown(ueberlagerung!, {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });

    expect(onVerwerfen).not.toHaveBeenCalled();
    expect(onUebernehmen).not.toHaveBeenCalled();
    expect(screen.getByTestId("nachfrage-bestaetigen")).toBeTruthy();
  });

  it("sperrt die Bestaetigung waehrend des Speicherns", () => {
    bauen({ laeuft: true });

    expect(
      (screen.getByTestId("nachfrage-bestaetigen") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });
});
