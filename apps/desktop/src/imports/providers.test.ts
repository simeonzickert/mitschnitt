import { describe, expect, it } from "vitest";

import {
  detectMeetingImportProviders,
  MEETING_IMPORT_PROVIDERS,
} from "./providers";

describe("meeting import providers", () => {
  it("keeps every researched provider in the catalog", () => {
    expect(MEETING_IMPORT_PROVIDERS).toHaveLength(32);
    expect(
      new Set(MEETING_IMPORT_PROVIDERS.map((provider) => provider.id)).size,
    ).toBe(MEETING_IMPORT_PROVIDERS.length);
  });

  it("enables direct imports for MCP and CLI providers", () => {
    expect(
      MEETING_IMPORT_PROVIDERS.filter((provider) => provider.directImport).map(
        (provider) => provider.id,
      ),
    ).toEqual([
      "granola",
      "circleback",
      "fireflies",
      "krisp",
      "read-ai",
      "fellow",
      "tactiq",
      "jiminny",
      "plaud",
      "pocket",
    ]);
    expect(
      MEETING_IMPORT_PROVIDERS.find((provider) => provider.id === "plaud"),
    ).toMatchObject({
      directImport: "cli",
    });
    expect(
      MEETING_IMPORT_PROVIDERS.find((provider) => provider.id === "pocket"),
    ).toMatchObject({
      directImport: "mcp-oauth",
      helpUrl: "https://docs.heypocketai.com/docs",
    });
    expect(
      MEETING_IMPORT_PROVIDERS.find((provider) => provider.id === "zoom")
        ?.directImport,
    ).toBeUndefined();
  });

  it("detects exact native names and bundle identifiers", () => {
    const providers = detectMeetingImportProviders([
      { id: "com.granola.app", name: "Granola" },
      { id: "ai.plaud.desktop.plaud", name: "Plaud Desktop" },
      { id: "com.microsoft.teams2", name: "Microsoft Teams" },
      { id: "com.openvisionengineering.pocket-desktop-app", name: "Pocket" },
    ]);

    expect(providers.map((provider) => provider.id)).toEqual([
      "anarlog",
      "granola",
      "plaud",
      "pocket",
      "microsoft-teams",
      "google-meet",
    ]);
    expect(providers.map((provider) => provider.installedAppId)).toEqual([
      "anarlog",
      "com.granola.app",
      "ai.plaud.desktop.plaud",
      "com.openvisionengineering.pocket-desktop-app",
      "com.microsoft.teams2",
      "google-meet",
    ]);
  });

  it("detects Plaud and Pocket desktop apps from Windows display names", () => {
    const providers = detectMeetingImportProviders([
      { id: "windows:hklm:Plaud Desktop", name: "Plaud Desktop" },
      { id: "windows:hkcu:Pocket", name: "Pocket Desktop" },
    ]);

    expect(providers.map((provider) => provider.id)).toEqual([
      "anarlog",
      "plaud",
      "pocket",
      "google-meet",
    ]);
  });

  it("does not treat Pocket Casts as Pocket", () => {
    expect(
      detectMeetingImportProviders([
        { id: "com.electron.pocket-casts", name: "Pocket Casts" },
      ]).map((provider) => provider.id),
    ).toEqual(["anarlog", "google-meet"]);
  });

  it("does not accept bundle identifier prefixes", () => {
    expect(
      detectMeetingImportProviders([
        { id: "com.granola.app.helper", name: "Something Else" },
      ]).map((provider) => provider.id),
    ).toEqual(["anarlog", "google-meet"]);
  });

  it("does not infer extension-only products from a browser", () => {
    expect(
      detectMeetingImportProviders([
        { id: "com.google.Chrome", name: "Google Chrome" },
      ]).map((provider) => provider.id),
    ).toEqual(["anarlog", "google-meet"]);
  });
});
