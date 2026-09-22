import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  platform: vi.fn(() => "macos"),
  setAppIcon: vi.fn(),
  applyAppIconPreference: vi.fn(),
  appIcon: "default",
  theme: "system",
  appIdentifier: "media.zickert.mitschnitt.stable" as string | undefined,
  billing: {
    isPro: true,
    isUpgradingToPro: false,
    upgradeToPro: vi.fn(),
  },
}));

vi.mock("@tanstack/react-query", () => ({
  useQuery: () => ({ data: mocks.appIdentifier }),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getIdentifier: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: mocks.platform,
}));

vi.mock("~/settings/queries", () => ({
  useSetSettingValue: () => mocks.setAppIcon,
}));

vi.mock("~/shared/config", () => ({
  useConfigValue: (key: string) =>
    key === "theme" ? mocks.theme : mocks.appIcon,
}));

vi.mock("~/shared/theme/provider", () => ({
  applyAppIconPreference: mocks.applyAppIconPreference,
}));

import { AppIconSelector } from "./app-icon";

describe("AppIconSelector", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    mocks.platform.mockReturnValue("macos");
    mocks.appIcon = "default";
    mocks.theme = "system";
    mocks.appIdentifier = "media.zickert.mitschnitt.stable";
    mocks.billing.isPro = true;
    mocks.billing.isUpgradingToPro = false;
  });

  const iconOptions = () =>
    within(screen.getByRole("radiogroup", { name: "App icon" })).getAllByRole(
      "radio",
    );

  // ZICK-278 (02.09.2026): es bleibt nur die Welle. Im Stable-Bau ist
  // "Production" gleich "Default", also gibt es genau eine Wahl -- und ein
  // Waehler mit einer Option ist keiner. Der Abschnitt bleibt dann weg;
  // der Code-Pfad bleibt fuer kuenftige freigegebene Varianten.
  it("renders nothing when only the default icon remains", () => {
    render(<AppIconSelector />);

    expect(screen.queryByRole("radiogroup", { name: "App icon" })).toBeNull();
    expect(screen.queryByText("App icon")).toBeNull();
  });

  it("offers the production icon next to the channel default on staging", () => {
    mocks.appIdentifier = "media.zickert.mitschnitt.staging";

    render(<AppIconSelector />);

    expect(iconOptions()).toHaveLength(2);
    const defaultOption = screen.getByRole("radio", { name: "Default" });
    expect(defaultOption.getAttribute("aria-checked")).toBe("true");
    expect(defaultOption.querySelector("source")).toBeNull();
    expect(defaultOption.querySelector("img")?.getAttribute("src")).toBe(
      "/assets/app-icons/staging.png",
    );
    const production = screen.getByRole("radio", { name: "Production" });
    expect(production.querySelector("img")?.getAttribute("src")).toBe(
      "/assets/app-icons/stable.png",
    );
    for (const gone of [
      "Anagram",
      "Blueprint",
      "Sketch",
      "Field Journal",
      "Notepad",
      "Stone",
      "Typewriter Key",
      "Walnut",
    ]) {
      expect(screen.queryByRole("radio", { name: gone })).toBeNull();
    }

    fireEvent.click(production);

    expect(mocks.applyAppIconPreference).toHaveBeenCalledWith(
      "stable",
      "system",
    );
    expect(mocks.setAppIcon).toHaveBeenCalledWith("stable");
  });

  it("keeps the same preview for an explicit dark theme", () => {
    mocks.appIdentifier = "media.zickert.mitschnitt.staging";
    mocks.theme = "dark";

    render(<AppIconSelector />);

    const defaultOption = screen.getByRole("radio", { name: "Default" });
    expect(
      defaultOption.querySelector(
        'source[media="(prefers-color-scheme: dark)"]',
      ),
    ).toBeNull();
    expect(defaultOption.querySelector("img")?.getAttribute("src")).toBe(
      "/assets/app-icons/staging.png",
    );
  });

  // Eine gespeicherte Wahl eines entfernten Icons (hier "walnut") landet
  // auf "Default" -- kein fehlendes Bild, kein leerer Waehler.
  it("selects default for a stored choice of a removed icon", () => {
    mocks.appIdentifier = "media.zickert.mitschnitt.staging";
    mocks.appIcon = "walnut";

    render(<AppIconSelector />);

    expect(
      screen
        .getByRole("radio", { name: "Default" })
        .getAttribute("aria-checked"),
    ).toBe("true");
  });

  // E4 c (Opus): seit ZICK-278 ist "stable" im Staging-Bau die Gegenwahl
  // zum Kanal-Standard, nicht dessen Aequivalent -- der alte Name des Tests
  // ("selects default for an equivalent channel-specific preference") sagte
  // das Gegenteil von dem, was er prueft.
  it("selects Production for a stored stable preference on staging", () => {
    mocks.appIcon = "stable";
    mocks.appIdentifier = "media.zickert.mitschnitt.staging";

    render(<AppIconSelector />);

    expect(
      screen
        .getByRole("radio", { name: "Production" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(
      screen
        .getByRole("radio", { name: "Default" })
        .getAttribute("aria-checked"),
    ).toBe("false");
  });

  // Im Stable-Bau IST "stable" der Standard: eine gespeicherte Wahl darauf
  // eroeffnet keine zweite Option, der Abschnitt bleibt unsichtbar.
  it("stays hidden on stable even with a stored stable preference", () => {
    mocks.appIcon = "stable";
    mocks.appIdentifier = "media.zickert.mitschnitt.stable";

    render(<AppIconSelector />);

    expect(screen.queryByRole("radiogroup", { name: "App icon" })).toBeNull();
    expect(screen.queryByText("App icon")).toBeNull();
  });

  it("uses the stable icon while the app identifier is loading", () => {
    mocks.appIdentifier = undefined;

    render(<AppIconSelector />);

    expect(screen.queryByRole("radiogroup", { name: "App icon" })).toBeNull();
  });

  it("is hidden on platforms that cannot change the running app icon", () => {
    mocks.platform.mockReturnValue("windows");

    render(<AppIconSelector />);

    expect(screen.queryByText("App icon")).toBeNull();
  });
});
