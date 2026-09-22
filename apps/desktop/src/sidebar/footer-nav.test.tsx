import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  currentTab: null as { type: string } | null,
  openNew: vi.fn(),
}));

vi.mock("~/store/zustand/tabs", () => ({
  useTabs: (
    selector: (state: {
      currentTab: typeof mocks.currentTab;
      openNew: typeof mocks.openNew;
    }) => unknown,
  ) => selector({ currentTab: mocks.currentTab, openNew: mocks.openNew }),
}));

import { SidebarFooterNav } from "./footer-nav";

// Bewusst direkt aus dem Schema-Modul, nicht aus `~/store/zustand/tabs` - das
// ist oben gemockt, und ein gemockter Wahrheitspruefer prueft nichts.
import { isTabInputSupported } from "~/store/zustand/tabs/schema";

describe("SidebarFooterNav", () => {
  beforeEach(() => {
    mocks.currentTab = null;
    mocks.openNew.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  // Rot, sobald ein Knopf verschwindet, umbenannt wird oder die Reihenfolge
  // kippt - Einstellungen muss der letzte Eintrag bleiben (Vorgabe).
  it("renders the nav buttons in order, settings last", () => {
    render(<SidebarFooterNav />);

    const labels = screen
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label"));

    expect(labels).toEqual(["Calendar", "Contacts", "Settings"]);
  });

  // Rot, sobald ein Klick nirgendwohin fuehrt oder das falsche Ziel oeffnet -
  // genau der Falsifikator aus dem F15-Claim.
  it.each([
    ["Calendar", { type: "calendar" }],
    ["Contacts", { type: "contacts" }],
    ["Settings", { type: "settings" }],
  ] as const)("opens %s", (label, destination) => {
    render(<SidebarFooterNav />);

    fireEvent.click(screen.getByLabelText(label));

    expect(mocks.openNew).toHaveBeenCalledTimes(1);
    expect(mocks.openNew).toHaveBeenCalledWith(destination);
  });

  // Rot, sobald der aktive Zustand fehlt oder auf dem falschen Knopf sitzt.
  it.each([
    ["calendar", "Calendar"],
    ["contacts", "Contacts"],
    ["settings", "Settings"],
  ] as const)("marks %s as the active entry", (tabType, activeLabel) => {
    mocks.currentTab = { type: tabType };
    render(<SidebarFooterNav />);

    for (const label of ["Calendar", "Contacts", "Settings"]) {
      const button = screen.getByLabelText(label);
      const shouldBeActive = label === activeLabel;

      expect(button.dataset.active).toBe(String(shouldBeActive));
      expect(button.getAttribute("aria-current")).toBe(
        shouldBeActive ? "page" : null,
      );
    }
  });

  // Rot, sobald ein unbeteiligter Tab faelschlich einen Knopf aktiv macht.
  it("marks nothing active on an unrelated tab", () => {
    mocks.currentTab = { type: "sessions" };
    render(<SidebarFooterNav />);

    for (const button of screen.getAllByRole("button")) {
      expect(button.dataset.active).toBe("false");
    }
  });

  // Rot, sobald ein Knopf ohne echtes Ziel eingebaut wird (z. B. ein
  // Ordner-Knopf, der auf einen Tab-Typ zeigt, den die App nicht rendert).
  //
  // Der Kommentar versprach das schon vorher, der Test pruefte aber nur, DASS
  // `openNew` gerufen wurde - ein Knopf mit `{ type: "folders" }` haette ihn
  // gruen gelassen. Jetzt laeuft jedes Ziel durch `isTabInputSupported`, also
  // durch genau die Typmenge, die `store/zustand/tabs/schema.ts` zulaesst.
  it("has no button whose destination the app cannot render", () => {
    render(<SidebarFooterNav />);
    const buttons = screen.getAllByRole("button");

    for (const button of buttons) {
      mocks.openNew.mockClear();
      fireEvent.click(button);
      expect(mocks.openNew).toHaveBeenCalledTimes(1);

      const [destination] = mocks.openNew.mock.calls[0] as [
        { type: string } | undefined,
      ];
      expect(destination).toBeDefined();
      expect(
        isTabInputSupported(
          destination as Parameters<typeof isTabInputSupported>[0],
        ),
        `the "${button.getAttribute("aria-label")}" button opens "${destination?.type}", which the app does not render`,
      ).toBe(true);
    }
  });
});
