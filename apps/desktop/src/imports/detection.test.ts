import { beforeEach, describe, expect, it, vi } from "vitest";

const {
  hasKnownImportSource,
  getInstalledApplicationIcons,
  listInstalledApplications,
} = vi.hoisted(() => ({
  hasKnownImportSource: vi.fn(),
  getInstalledApplicationIcons: vi.fn(),
  listInstalledApplications: vi.fn(),
}));

vi.mock("@anlg/plugin-detect", () => ({
  commands: { getInstalledApplicationIcons, listInstalledApplications },
}));
vi.mock("./anarlog-contract", () => ({ hasKnownImportSource }));

import { detectImportSources } from "./detection";

describe("import source detection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getInstalledApplicationIcons.mockResolvedValue({
      status: "ok",
      data: [
        {
          id: "com.granola.app",
          dataUrl: "data:image/png;base64,granola",
        },
      ],
    });
    listInstalledApplications.mockResolvedValue({
      status: "ok",
      data: [{ id: "com.granola.app", name: "Granola" }],
    });
    // Default: die billige Selbstsuche findet nichts -- die anarlog-Zeile
    // bleibt aus.
    hasKnownImportSource.mockResolvedValue(false);
  });

  it("detects installed import sources without terminating them", async () => {
    const result = await detectImportSources();

    expect(listInstalledApplications).toHaveBeenCalledOnce();
    expect(getInstalledApplicationIcons).toHaveBeenCalledWith([
      "import-folder",
      "com.granola.app",
      "google-meet",
    ]);
    expect(result.map((provider) => provider.id)).toEqual([
      "import-folder",
      "granola",
      "google-meet",
    ]);
    expect(result.find((provider) => provider.id === "granola")?.iconUrl).toBe(
      "data:image/png;base64,granola",
    );
  });

  it("always offers the brandless folder import, fund or not", async () => {
    const result = await detectImportSources();
    expect(result.some((provider) => provider.id === "import-folder")).toBe(
      true,
    );
  });

  it("hides the anarlog folder source when the self-search finds nothing", async () => {
    hasKnownImportSource.mockResolvedValue(false);

    const result = await detectImportSources();

    expect(result.some((provider) => provider.id === "anarlog")).toBe(false);
  });

  it("shows the anarlog folder source once the cheap self-search reports a fund", async () => {
    hasKnownImportSource.mockResolvedValue(true);

    const result = await detectImportSources();

    expect(result.map((provider) => provider.id)).toEqual([
      "anarlog",
      "import-folder",
      "granola",
      "google-meet",
    ]);
    // Die Ordner-Quelle hat kein Bundle und deshalb kein OS-Symbol.
    expect(
      result.find((provider) => provider.id === "anarlog")?.iconUrl,
    ).toBeUndefined();
  });

  it("treats a failed self-search as no fund instead of failing the whole detection", async () => {
    hasKnownImportSource.mockRejectedValue(new Error("boom"));

    const result = await detectImportSources();

    expect(result.some((provider) => provider.id === "anarlog")).toBe(false);
    expect(result.map((provider) => provider.id)).toEqual([
      "import-folder",
      "granola",
      "google-meet",
    ]);
  });
});
