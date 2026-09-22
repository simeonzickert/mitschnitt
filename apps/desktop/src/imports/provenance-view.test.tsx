import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  useSessionImportProvenance: vi.fn(),
  useSessionTimingQuality: vi.fn(),
}));

vi.mock("./provenance-queries", () => ({
  useSessionImportProvenance: mocks.useSessionImportProvenance,
  useSessionTimingQuality: mocks.useSessionTimingQuality,
}));

import {
  ImportedMarker,
  importSourceLabel,
  SessionImportNote,
  TranscriptTimingNotice,
} from "./provenance-view";

describe("importSourceLabel", () => {
  it("turns the stored key into the name the person knows", () => {
    expect(importSourceLabel("anarlog")).toBe("anarlog / Hyprnote");
    expect(importSourceLabel("microsoft-teams")).toBe("Microsoft Teams");
  });

  // Lieber roh als verschwiegen: eine unbekannte Kennung ist immer noch eine
  // Antwort auf "woher kam das".
  it("shows an unknown key as it is instead of dropping it", () => {
    expect(importSourceLabel("some-new-app")).toBe("some-new-app");
    expect(importSourceLabel(null)).toBeNull();
  });
});

describe("ImportedMarker", () => {
  afterEach(cleanup);

  it("carries source and day in its label", () => {
    render(
      <ImportedMarker
        sourceApp="anarlog"
        importedAt="2026-09-10T08:00:00.000Z"
      />,
    );

    expect(
      screen.getByLabelText("Imported from anarlog / Hyprnote on Sep 10, 2026"),
    ).toBeTruthy();
  });

  // Der Datei-Import schreibt keinen Zeitpunkt. Das darf den Merker nicht
  // kaputtmachen und erst recht kein erfundenes Datum erzeugen.
  it("leaves the day out when the import did not record one", () => {
    render(<ImportedMarker sourceApp="fathom" importedAt={null} />);

    expect(screen.getByLabelText("Imported from fathom")).toBeTruthy();
  });

  it("survives an import that recorded a date nobody can parse", () => {
    render(<ImportedMarker sourceApp="fathom" importedAt="whenever" />);

    expect(screen.getByLabelText("Imported from fathom")).toBeTruthy();
  });
});

describe("SessionImportNote", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(cleanup);

  it("says nothing at all for a session nobody imported", () => {
    mocks.useSessionImportProvenance.mockReturnValue(null);

    const { container } = render(<SessionImportNote sessionId="s1" />);

    expect(container.textContent).toBe("");
  });

  it("names where it came from and when", () => {
    mocks.useSessionImportProvenance.mockReturnValue({
      sourceApp: "anarlog",
      sourceRoot: "/Users/example/somewhere",
      importedAt: "2026-09-10T08:00:00.000Z",
    });

    render(<SessionImportNote sessionId="s1" />);

    expect(
      screen.getByText(/Brought over from anarlog \/ Hyprnote on Sep 10, 2026/),
    ).toBeTruthy();
    expect(screen.getByText("/Users/example/somewhere")).toBeTruthy();
  });

  it("still says it was imported when the source is unnamed", () => {
    mocks.useSessionImportProvenance.mockReturnValue({
      sourceApp: null,
      sourceRoot: null,
      importedAt: null,
    });

    render(<SessionImportNote sessionId="s1" />);

    expect(screen.getByText(/Brought over from another app/)).toBeTruthy();
  });
});

describe("TranscriptTimingNotice", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(cleanup);

  it("warns that a click will not land on the word", () => {
    mocks.useSessionTimingQuality.mockReturnValue("synthetic");

    render(<TranscriptTimingNotice sessionId="s1" />);

    const notice = screen.getByTestId("transcript-timing-notice");
    expect(notice.textContent).toContain("estimated, not measured");
    expect(notice.textContent).toContain("will not land on it exactly");
  });

  it("says only some of them when the transcripts disagree", () => {
    mocks.useSessionTimingQuality.mockReturnValue("mixed");

    expect(
      render(<TranscriptTimingNotice sessionId="s1" />).container.textContent,
    ).toContain("Some of the word times");
  });

  // Der teure Fehler waere andersherum: eine Warnung ueber gemessenen Zeiten
  // macht den Abspieler unglaubwuerdig, obwohl er stimmt.
  it("stays quiet for measured times and for an unknown verdict", () => {
    for (const quality of ["provider", null]) {
      mocks.useSessionTimingQuality.mockReturnValue(quality);
      const { container, unmount } = render(
        <TranscriptTimingNotice sessionId="s1" />,
      );
      expect(container.textContent).toBe("");
      unmount();
    }
  });
});
