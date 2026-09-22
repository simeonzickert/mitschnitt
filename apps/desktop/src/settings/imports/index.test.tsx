import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openUrl: vi.fn(),
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openUrl: mocks.openUrl },
}));

vi.mock("~/imports/screen", () => ({
  MeetingImportScreen: () => <div>Import list</div>,
}));

import { SettingsImports } from ".";

describe("SettingsImports", () => {
  afterEach(cleanup);

  it("renders the import list", () => {
    render(<SettingsImports />);

    expect(screen.getByText("Import list")).toBeTruthy();
  });

  // Regressions-Wache: hier stand ein "Documentation"-Knopf auf
  // docs.anarlog.so/imports. Der Test wird rot, sobald diese Seite wieder
  // irgendeinen Knopf bekommt, der eine Adresse oeffnet -- egal welche.
  it("opens no external documentation from this page", () => {
    render(<SettingsImports />);

    expect(screen.queryByRole("button")).toBeNull();
    expect(mocks.openUrl).not.toHaveBeenCalled();
  });
});
