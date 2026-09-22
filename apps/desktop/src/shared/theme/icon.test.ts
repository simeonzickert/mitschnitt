import { describe, expect, it } from "vitest";

import {
  hasDarkAppIconVariant,
  normalizeAppIconPreference,
  resolveAppIconName,
  resolveDockIconName,
} from "./icon";

describe("app icon preference", () => {
  it("falls back to the default icon for unknown values", () => {
    expect(normalizeAppIconPreference(undefined)).toBe("default");
    expect(normalizeAppIconPreference("unknown")).toBe("default");
    expect(normalizeAppIconPreference("stable")).toBe("stable");
  });

  // ZICK-278 (02.09.2026): die geerbten Icons sind weg. Eine Einstellung,
  // die noch auf eines davon zeigt, faellt auf die Welle zurueck statt ein
  // fehlendes Bild zu laden -- Rueckweg-Muster wie bei 288576e7a0.
  it("maps a stored choice of a removed icon back to the default", () => {
    for (const removed of [
      "anagram",
      "dev",
      "staging",
      "journal",
      "notepad",
      "stone",
      "typewriter-key",
      "walnut",
    ]) {
      expect(normalizeAppIconPreference(removed)).toBe("default");
    }
  });

  it("resolves the default icon from the app channel", () => {
    expect(
      resolveAppIconName("default", "media.zickert.mitschnitt.stable"),
    ).toBe("stable");
    expect(
      resolveAppIconName("default", "media.zickert.mitschnitt.staging"),
    ).toBe("staging");
    // Der Dev-Bau traegt die nackte Kennung ohne ".dev"-Suffix und landet
    // damit auf dem stable-Slot -- unveraendert seit dem Fork; sichtbar ist
    // kein Unterschied, alle Kanal-Dateien tragen dieselbe Welle.
    expect(resolveAppIconName("default", "media.zickert.mitschnitt")).toBe(
      "stable",
    );
    expect(
      resolveAppIconName("stable", "media.zickert.mitschnitt.staging"),
    ).toBe("stable");
  });

  // Die Welle ist in beiden Erscheinungsbildern dieselbe Datei; eine dunkle
  // Fassung gibt es erst mit einer Freigabe. Der Mechanismus bleibt, die
  // Liste der Icons mit Dunkel-Variante ist leer.
  it("uses the same Dock icon in light and dark appearance", () => {
    for (const name of ["stable", "dev", "staging"] as const) {
      expect(hasDarkAppIconVariant(name)).toBe(false);
    }
    expect(
      resolveDockIconName(
        "default",
        "system",
        true,
        "media.zickert.mitschnitt.stable",
      ),
    ).toBe("stable");
    expect(
      resolveDockIconName(
        "default",
        "dark",
        false,
        "media.zickert.mitschnitt.stable",
      ),
    ).toBe("stable");
    expect(
      resolveDockIconName(
        "default",
        "system",
        true,
        "media.zickert.mitschnitt.staging",
      ),
    ).toBe("staging");
    expect(
      resolveDockIconName("stable", "dark", true, "media.zickert.mitschnitt"),
    ).toBe("stable");
  });
});
