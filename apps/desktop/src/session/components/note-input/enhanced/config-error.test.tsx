import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const openNew = vi.hoisted(() => vi.fn());

vi.mock("~/store/zustand/tabs", () => ({
  useTabs: (selector: (state: { openNew: typeof openNew }) => unknown) =>
    selector({ openNew }),
}));

import { ConfigError } from "./config-error";

describe("ConfigError", () => {
  afterEach(() => {
    cleanup();
    openNew.mockReset();
  });

  it("sends the reader to the Intelligence settings", () => {
    render(<ConfigError />);

    expect(screen.getByRole("alert")).not.toBeNull();
    expect(screen.getByText("Set up AI summaries")).not.toBeNull();

    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Open Intelligence settings" }),
    );
    expect(openNew).toHaveBeenCalledTimes(1);
    expect(openNew).toHaveBeenCalledWith({
      type: "settings",
      state: { tab: "intelligence" },
    });
  });

  // Mitschnitt-Fork. Dieser Bildschirm erscheint auf einer frischen
  // Installation beim ersten Meeting -- also genau das, was ein neuer Mensch
  // als erstes sieht. Bis zum 04.09.2026 stand hier ein Knopf "Get Pro", der
  // auf den Einstellungs-Reiter "account" zeigte. Den Reiter gibt es in diesem
  // Fork nicht (`SettingsView` hat keinen `case "account"` und faellt stumm
  // auf die allgemeine Seite), und ein Abo gibt es auch nicht. Der Test haelt
  // beide Haelften fest: kein Versprechen, kein totes Ziel.
  it("promises no subscription and never opens the account tab", () => {
    render(<ConfigError />);

    // queryAllByText, nicht queryByText: bei mehreren Treffern wirft die
    // Einzahl-Fassung einen Werkzeugfehler ("multiple elements found") statt
    // die Zusicherung scheitern zu lassen. Der Test waere dann zwar rot, aber
    // aus dem falschen Grund -- und damit kein Beweis. Am Mutanten geprueft.
    expect(screen.queryAllByText(/Pro/)).toHaveLength(0);
    expect(screen.queryAllByText(/trial/i)).toHaveLength(0);

    for (const button of screen.getAllByRole("button")) {
      fireEvent.click(button);
    }
    for (const [ziel] of openNew.mock.calls) {
      expect(ziel.state.tab).not.toBe("account");
    }
  });
});
