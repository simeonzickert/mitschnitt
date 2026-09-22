import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getIdentifier: vi.fn(),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getIdentifier: mocks.getIdentifier,
}));

import { getScheme } from "./utils";

describe("getScheme", () => {
  beforeEach(() => {
    mocks.getIdentifier.mockReset();
  });

  it.each([
    ["media.zickert.mitschnitt", "mitschnitt"],
    ["media.zickert.mitschnitt.stable", "mitschnitt"],
    ["media.zickert.mitschnitt.staging", "mitschnitt"],
    ["media.zickert.mitschnitt.desktop", "mitschnitt"],
    ["media.zickert.mitschnitt.flatpak", "mitschnitt"],
  ])("maps %s to %s", async (identifier, scheme) => {
    mocks.getIdentifier.mockResolvedValue(identifier);

    await expect(getScheme()).resolves.toBe(scheme);
  });

  // Der eigentliche Punkt: KEINE Identitaet - auch keine unbekannte, auch keine
  // aus dem Upstream - darf je ein fremdes Schema erzeugen. Der Vorgaengertest
  // prueft genau das Gegenteil ab ("unknown" -> "anarlog") und haette den Bug
  // fuer immer als Sollverhalten festgeschrieben (R10: der Test prueft die
  // Formulierung, nicht die Sache). Faellt dieser Test, zeigt der Rueckkanal
  // wieder auf eine fremde Installation.
  it.each([
    ["unknown"],
    ["com.hyprnote.stable"],
    ["so.anarlog.Anarlog"],
    ["com.anarlog.staging"],
  ])("never resolves %s to a foreign scheme", async (identifier) => {
    mocks.getIdentifier.mockResolvedValue(identifier);

    const scheme = await getScheme();

    expect(scheme).toBe("mitschnitt");
    expect(scheme).not.toMatch(/anarlog|hyprnote|hypr/);
  });
});
