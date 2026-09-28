import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
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
// dass er ueberhaupt mit der richtigen Sitzung eingesetzt ist -- sein eigenes
// Verhalten pruefen seine eigenen Tests, an seinem eigenen Ort
// (`participants/dropdown.test.tsx`).
//
// Die Attrappe traegt trotzdem die zwei Merkmale, von denen der Dialog
// abhaengt, gesteuert ueber Modul-Flags -- so laesst sich am Dialog selbst
// pruefen, ohne die echte, schwergewichtige Eingabe nachzubauen:
//
// - `data-participant-suggestions-open` sitzt NICHT portaliert (echter
//   DOM-Nachkomme dieses Blocks, wie die Eingabezeile in `input.tsx`) --
//   dafuer, dass die Escape-Weiche des Dialogs eine offene Liste erkennt.
// - `data-participant-suggestions` sitzt an einem echten `createPortal`
//   nach `document.body` (wie `dropdown.tsx`s FloatingPortal) -- dafuer,
//   dass ein Klick auf einen Vorschlag den Dialog nicht als "Klick daneben"
//   ueberspringen laesst (D1, siehe Bericht).
const mocks = vi.hoisted(() => ({
  vorschlagslisteOffen: false,
  vorschlagPortal: null as { onSelect: () => void } | null,
}));

vi.mock("~/session/components/outer-header/metadata/participants", () => ({
  ParticipantsDisplay: ({ sessionId }: { sessionId: string }) => (
    <div data-testid="teilnehmer-block" data-session={sessionId}>
      {mocks.vorschlagslisteOffen && <div data-participant-suggestions-open />}
      {mocks.vorschlagPortal &&
        createPortal(
          <button
            type="button"
            data-participant-suggestions
            data-testid="vorschlag-attrappe"
            onClick={mocks.vorschlagPortal.onSelect}
          >
            Anna Beispiel
          </button>,
          document.body,
        )}
    </div>
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
  afterEach(() => {
    cleanup();
    mocks.vorschlagslisteOffen = false;
    mocks.vorschlagPortal = null;
    document
      .querySelectorAll("[data-participant-suggestions-open]")
      .forEach((knoten) => knoten.remove());
  });

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

    expect(screen.getByText("Continue")).toBeTruthy();
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

  // Die Frage endet mit einer Entscheidung, nie mit nichts -- aber die
  // Entscheidung darf seit 26.09.2026 auch "ueberspringen" heissen. Bis dahin
  // schloss Escape den Dialog lautlos gar nicht (verbindlich); jetzt ist
  // Escape einer von vier Wegen zum Ueberspringen (Knopf, X, Escape, Klick
  // daneben), alle auf `onVerwerfen`.
  it("ueberspringt bei Escape, wenn keine Vorschlagsliste offen ist", () => {
    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: "Escape",
    });

    expect(onVerwerfen).toHaveBeenCalledTimes(1);
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  // Die Vorschlagsliste bekommt Escape zuerst: ein einzelner Escape-Druck
  // darf nie beides auf einmal tun (Liste schliessen UND ueberspringen). Der
  // Dialog erkennt eine offene Liste am Merkmal
  // `data-participant-suggestions-open` an der (nicht portalierten)
  // Eingabezeile (siehe Kommentar in `dialog.tsx`, warum das nicht ueber
  // `stopPropagation` in der Liste selbst geht).
  it("ueberspringt nicht, solange die eigene Vorschlagsliste offen ist", () => {
    mocks.vorschlagslisteOffen = true;
    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.keyDown(document.body, { key: "Escape" });

    expect(onVerwerfen).not.toHaveBeenCalled();
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  // D2 (Zweitblick 26.09.2026): die Escape-Weiche war ein GLOBALES
  // `document.querySelector` ohne Bezug zu diesem Dialog. Eine Liste, die
  // irgendwo GANZ ANDERS im Dokument offen ist (z. B. das
  // Kopfzeilen-Popover einer anderen, im Hintergrund liegenden Sitzung),
  // haette Escape hier faelschlich blockiert. Seit dem Fix ist die Abfrage
  // auf `inhaltRef.current` gescopt und findet ein Merkmal ausserhalb
  // dieses Dialogs nicht mehr.
  it("laesst sich von einer FREMDEN, anderswo offenen Liste nicht blockieren (D2)", () => {
    const fremd = document.createElement("div");
    fremd.setAttribute("data-participant-suggestions-open", "");
    document.body.appendChild(fremd);

    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.keyDown(document.body, { key: "Escape" });

    expect(onVerwerfen).toHaveBeenCalledTimes(1);
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  // D1 (Zweitblick 26.09.2026), Rot-Beweis: ein modaler Radix-Dialog setzt
  // `document.body.style.pointerEvents = "none"` und gibt es nur seinem
  // eigenen Content und dem Overlay explizit wieder frei
  // (`@radix-ui/react-dismissable-layer`, `@radix-ui/react-dialog`,
  // installierter Quelltext). Die Vorschlagsliste haengt per FloatingPortal
  // NEBEN diesem Dialog direkt am <body> -- ohne `pointer-events-auto` fiel
  // ein Klick auf sie durch auf den Overlay darunter, der ihn als "Klick
  // daneben" las: der Dialog ueberspringt, ohne dass die Auswahl je ankommt.
  // Diese Attrappe bildet genau diese Anordnung nach (echtes `createPortal`
  // nach `document.body`, wie `dropdown.tsx`s FloatingPortal).
  it("waehlt einen Vorschlag aus der portalierten Liste, ohne dass der Dialog ueberspringt (D1)", async () => {
    const onSelect = vi.fn();
    mocks.vorschlagPortal = { onSelect };
    const { onVerwerfen, onUebernehmen } = bauen();

    // Derselbe Tick wie beim Klick-daneben-Test unten: Radix haengt seinen
    // Zuhoerer fuer Klicks nach draussen erst per `setTimeout(0)` an.
    await new Promise((weiter) => setTimeout(weiter, 0));

    const vorschlag = screen.getByTestId("vorschlag-attrappe");
    fireEvent.pointerDown(vorschlag, {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });
    fireEvent.click(vorschlag);

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onVerwerfen).not.toHaveBeenCalled();
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  it("ueberspringt bei einem Klick daneben", async () => {
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

    expect(onVerwerfen).toHaveBeenCalledTimes(1);
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  it("ueberspringt ueber den Skip-Knopf, ohne zu speichern", () => {
    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.click(screen.getByTestId("nachfrage-ueberspringen"));

    expect(onVerwerfen).toHaveBeenCalledTimes(1);
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  it("ueberspringt ueber das Schliessen-X", () => {
    const { onVerwerfen, onUebernehmen } = bauen();

    fireEvent.click(screen.getByTestId("nachfrage-schliessen"));

    expect(onVerwerfen).toHaveBeenCalledTimes(1);
    expect(onUebernehmen).not.toHaveBeenCalled();
  });

  it("sperrt die Bestaetigung waehrend des Speicherns", () => {
    bauen({ laeuft: true });

    expect(
      (screen.getByTestId("nachfrage-bestaetigen") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  // Waehrend des Speicherns bleibt der Dialog verbindlich (siehe Kommentar in
  // dialog.tsx): der Confirm-Klick hat schon eine Antwort abgegeben, ein
  // Escape oder ein Klick daneben in genau diesem Fenster duerfte die
  // Zusammenfassung nicht VOR dem Titel-Schreiben freigeben.
  it("sperrt Skip und das Schliessen-X waehrend des Speicherns", () => {
    bauen({ laeuft: true });

    expect(
      (screen.getByTestId("nachfrage-ueberspringen") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(
      (screen.getByTestId("nachfrage-schliessen") as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("sperrt Escape waehrend des Speicherns", () => {
    const { onVerwerfen } = bauen({ laeuft: true });

    fireEvent.keyDown(document.body, { key: "Escape" });

    expect(onVerwerfen).not.toHaveBeenCalled();
  });

  it("sperrt einen Klick daneben waehrend des Speicherns", async () => {
    const { onVerwerfen } = bauen({ laeuft: true });

    await new Promise((weiter) => setTimeout(weiter, 0));
    const ueberlagerung = document.querySelector("[data-dialog-overlay]");
    fireEvent.pointerDown(ueberlagerung!, {
      button: 0,
      ctrlKey: false,
      pointerType: "mouse",
    });

    expect(onVerwerfen).not.toHaveBeenCalled();
  });
});
