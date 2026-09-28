import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children: ReactNode }) => children,
  useLingui: () => ({ t: (value: string) => value }),
}));

import { ParticipantDropdown } from "./dropdown";

// jsdom kennt `scrollIntoView` nicht (bekannte Luecke, kein Layout-Engine).
// Der Komponente fehlt dieser Bau bisher voellig -- ohne den Stub stirbt
// jeder Render an ihrem eigenen Scroll-Effekt, bevor ein Test ueberhaupt
// pruefen kann.
Element.prototype.scrollIntoView ??= () => {};

type Option = Parameters<typeof ParticipantDropdown>[0]["options"][number];

function bauen(
  optionen: Option[],
  overrides: Partial<Parameters<typeof ParticipantDropdown>[0]> = {},
) {
  const onSelect = vi.fn();
  const onHover = vi.fn();
  render(
    <ParticipantDropdown
      floatingRef={() => {}}
      floatingStyles={{}}
      options={optionen}
      selectedIndex={0}
      onSelect={onSelect}
      onHover={onHover}
      {...overrides}
    />,
  );
  return { onSelect, onHover };
}

describe("ParticipantDropdown", () => {
  afterEach(cleanup);

  it("zeigt nichts, solange keine Vorschlaege vorliegen", () => {
    const { container } = render(
      <ParticipantDropdown
        floatingRef={() => {}}
        floatingStyles={{}}
        options={[]}
        selectedIndex={0}
        onSelect={vi.fn()}
        onHover={vi.fn()}
      />,
    );

    expect(container.firstChild).toBeNull();
  });

  // Gemessen am Nachfrage-Dialog (26.09.2026): dessen Overlay UND Content
  // stehen auf z-50 (packages/ui/src/components/ui/dialog.tsx). Ohne eigenes
  // z-index verlor dieser Container immer -- unabhaengig von der
  // DOM-Reihenfolge, weil beide als eigene Portale direkt am <body> haengen.
  // Diese Pruefung bindet den Container an einen Wert, der garantiert ueber
  // jedem Dialog dieser App liegt.
  it("liegt ueber jedem Dialog-Overlay der App (z-[60] gegen deren z-50)", () => {
    render(
      <ParticipantDropdown
        floatingRef={() => {}}
        floatingStyles={{}}
        options={[{ id: "1", name: "Anna Beispiel" }]}
        selectedIndex={0}
        onSelect={vi.fn()}
        onHover={vi.fn()}
      />,
    );

    const container = screen
      .getByText("Anna Beispiel")
      .closest("[class*='z-']");
    expect(container?.className).toContain("z-[60]");
  });

  it("waehlt einen Vorschlag per Mausklick aus", () => {
    const option: Option = { id: "1", name: "Anna Beispiel" };
    const { onSelect } = bauen([option]);

    fireEvent.click(screen.getByText("Anna Beispiel"));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(option);
  });

  it("zeigt bei gleichem Anzeigenamen die Mailadresse zur Unterscheidung", () => {
    bauen([
      { id: "1", name: "Erika Beispiel | Beispielfirma", email: "erika@a.example" },
      { id: "2", name: "Erika Beispiel | Beispielfirma", email: "erika@b.example" },
    ]);

    expect(screen.getByText("erika@a.example")).toBeTruthy();
    expect(screen.getByText("erika@b.example")).toBeTruthy();
  });

  it("faellt ohne Mailadresse auf die Telefonnummer zurueck", () => {
    bauen([
      { id: "1", name: "Erika Beispiel | Beispielfirma", phone: "+49 1" },
      { id: "2", name: "Erika Beispiel | Beispielfirma", phone: "+49 2" },
    ]);

    expect(screen.getByText("+49 1")).toBeTruthy();
    expect(screen.getByText("+49 2")).toBeTruthy();
  });

  it("zeigt keine Unterscheidungszeile bei eindeutigem Namen", () => {
    bauen([
      { id: "1", name: "Anna Beispiel", email: "anna@example.com" },
      { id: "2", name: "Bea Beispiel", email: "bea@example.com" },
    ]);

    expect(screen.queryByText("anna@example.com")).toBeNull();
    expect(screen.queryByText("bea@example.com")).toBeNull();
  });

  it("zaehlt die angebotene 'Add ...'-Zeile nicht als Dublette", () => {
    bauen([
      { id: "new", name: "Anna Beispiel", isNew: true },
      { id: "1", name: "Anna Beispiel", email: "anna@example.com" },
    ]);

    expect(screen.queryByText("anna@example.com")).toBeNull();
  });

  // D4 (Zweitblick 26.09.2026): eine GETEILTE Mailadresse unterscheidet
  // nichts -- beide Zeilen wuerden denselben Text zeigen. Die Position
  // (jobTitle) steht schon UNBEDINGT inline daneben und unterscheidet hier
  // bereits von selbst -- keine zusaetzliche Unterzeile noetig, und wichtig:
  // "Vertrieb"/"Einkauf" duerfen deshalb nur EINMAL je Zeile stehen, nicht
  // zusaetzlich verdoppelt auf einer Unterzeile.
  it("ignoriert eine Mailadresse, die beide Dubletten teilen", () => {
    bauen([
      {
        id: "1",
        name: "Erika Beispiel | Beispielfirma",
        email: "erika@geteilt.example",
        jobTitle: "Vertrieb",
      },
      {
        id: "2",
        name: "Erika Beispiel | Beispielfirma",
        email: "erika@geteilt.example",
        jobTitle: "Einkauf",
      },
    ]);

    expect(screen.queryByText("erika@geteilt.example")).toBeNull();
    expect(screen.getAllByText("Vertrieb")).toHaveLength(1);
    expect(screen.getAllByText("Einkauf")).toHaveLength(1);
  });

  // D4: ohne Mail und Telefon reicht die inline ohnehin sichtbare,
  // unterschiedliche Position (jobTitle) allein zur Unterscheidung -- keine
  // zusaetzliche Unterzeile, keine Nummerierung noetig.
  it("nutzt eine inline schon unterschiedliche Position, ohne sie zu verdoppeln", () => {
    bauen([
      { id: "1", name: "Erika Beispiel | Beispielfirma", jobTitle: "Vertrieb" },
      { id: "2", name: "Erika Beispiel | Beispielfirma", jobTitle: "Einkauf" },
    ]);

    expect(screen.getAllByText("Vertrieb")).toHaveLength(1);
    expect(screen.getAllByText("Einkauf")).toHaveLength(1);
    expect(screen.queryByText("Erika Beispiel | Beispielfirma (1)")).toBeNull();
  });

  // D4: bleibt WIRKLICH nichts uebrig, das die beiden unterscheidet (auch
  // die Position teilen sie sich), zaehlt die Liste durch -- lieber "(1)"/
  // "(2)" als zwei optisch identische Zeilen.
  it("nummeriert durch, wenn nichts an den Dubletten unterscheidet", () => {
    bauen([
      { id: "1", name: "Erika Beispiel | Beispielfirma" },
      { id: "2", name: "Erika Beispiel | Beispielfirma" },
    ]);

    expect(screen.getByText("Erika Beispiel | Beispielfirma (1)")).toBeTruthy();
    expect(screen.getByText("Erika Beispiel | Beispielfirma (2)")).toBeTruthy();
  });

  // D4: eine GETEILTE Position ist ebenso wenig eine Unterscheidung wie eine
  // geteilte Mailadresse -- auch hier zaehlt die Liste durch.
  it("nummeriert durch, wenn auch die Position geteilt ist", () => {
    bauen([
      {
        id: "1",
        name: "Erika Beispiel | Beispielfirma",
        jobTitle: "Vertrieb",
      },
      {
        id: "2",
        name: "Erika Beispiel | Beispielfirma",
        jobTitle: "Vertrieb",
      },
    ]);

    expect(screen.getByText("Erika Beispiel | Beispielfirma (1)")).toBeTruthy();
    expect(screen.getByText("Erika Beispiel | Beispielfirma (2)")).toBeTruthy();
  });
});
