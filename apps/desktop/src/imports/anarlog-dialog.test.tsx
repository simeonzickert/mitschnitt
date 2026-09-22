import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  selectFolder: vi.fn(),
  findImportSources: vi.fn(),
  scanImportSource: vi.fn(),
  runImportWithProgress: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.selectFolder }));
vi.mock("./anarlog-contract", () => ({
  findImportSources: mocks.findImportSources,
  scanImportSource: mocks.scanImportSource,
}));
vi.mock("./anarlog-run", () => ({
  runImportWithProgress: mocks.runImportWithProgress,
}));

import type { ImportRunReport, ImportSourceScan } from "./anarlog-contract";
import { AnarlogImportDialog } from "./anarlog-dialog";

const GB = 1024 * 1024 * 1024;
const SOURCE = "/Users/example/Library/Application Support/other-app";

function scan(overrides: Partial<ImportSourceScan> = {}): ImportSourceScan {
  return {
    kind: "database",
    sourceRoot: SOURCE,
    sourceApp: "anarlog",
    sessionCount: 42,
    transcriptCount: 40,
    documentCount: 12,
    audioFileCount: 20,
    audioBytes: 2 * GB,
    freeBytesOnTarget: 60 * GB,
    oldestSessionAt: "2024-03-01T09:00:00.000Z",
    newestSessionAt: "2026-08-14T16:30:00.000Z",
    problems: [],
    ...overrides,
  };
}

function report(overrides: Partial<ImportRunReport> = {}): ImportRunReport {
  return {
    runId: "run-1",
    status: "completed",
    dryRun: true,
    // Zeilensummen ueber sechs Tabellen ...
    discovered: 252,
    imported: 240,
    skipped: 2,
    conflicts: 0,
    errors: 0,
    // ... und die Gespraechszahl, die der Mensch liest. Am gemessenen Bestand
    // liegen zwischen beiden Faktor sechs (2.355 Zeilen, 395 Gespraeche).
    conversationsDiscovered: 42,
    conversationsImported: 40,
    audioCopied: 0,
    audioBytesCopied: 0,
    startedAt: "2026-09-10T08:00:00.000Z",
    completedAt: "2026-09-10T08:00:03.000Z",
    error: null,
    ...overrides,
  };
}

function renderDialog() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <QueryClientProvider client={queryClient}>
      <AnarlogImportDialog
        open
        onOpenChange={() => {}}
        providerName="the other app"
      />
    </QueryClientProvider>,
  );
}

async function chooseFolder() {
  fireEvent.click(
    await screen.findByRole("button", { name: /Choose folder/i }),
  );
}

describe("AnarlogImportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.selectFolder.mockResolvedValue(SOURCE);
    mocks.runImportWithProgress.mockResolvedValue(report());
    mocks.findImportSources.mockResolvedValue([]);
  });

  afterEach(cleanup);

  // Betreiber, 11.09.2026: „Niemand weiss, wo der Folder liegt." Ohne diese beiden
  // Tests waere der Selbstfund-Weg vollstaendig ungeprueft -- und der Mock
  // lieferte still `undefined`, sodass jeder andere Test gruen blieb.
  it("zeigt einen selbst gefundenen Bestand, statt nach einem Pfad zu fragen", async () => {
    mocks.findImportSources.mockResolvedValue([scan({ sessionCount: 395 })]);
    mocks.scanImportSource.mockResolvedValue(scan({ sessionCount: 395 }));

    renderDialog();

    await screen.findByText(/Found on this Mac/i);
    expect(await screen.findByText(/395/)).toBeTruthy();
    expect(mocks.selectFolder).not.toHaveBeenCalled();
  });

  it("uebernimmt den angeklickten Fund als Quelle", async () => {
    mocks.findImportSources.mockResolvedValue([scan({ sessionCount: 395 })]);
    mocks.scanImportSource.mockResolvedValue(scan({ sessionCount: 395 }));

    renderDialog();

    fireEvent.click(await screen.findByText(/395/));

    await waitFor(() =>
      expect(mocks.scanImportSource).toHaveBeenCalledWith(SOURCE),
    );
  });

  it("explains a folder without any conversations instead of showing an error", async () => {
    mocks.scanImportSource.mockResolvedValue(
      scan({
        kind: "none",
        sourceApp: null,
        sessionCount: 0,
        transcriptCount: 0,
        documentCount: 0,
        audioFileCount: 0,
        audioBytes: 0,
        oldestSessionAt: null,
        newestSessionAt: null,
        problems: ["no_data_found"],
      }),
    );

    renderDialog();
    await chooseFolder();

    expect(
      await screen.findByText(/no conversations from another app/i),
    ).toBeTruthy();
    expect(
      screen.getByText(
        /Choose the folder that the other app stores its data in/i,
      ),
    ).toBeTruthy();
    // Kein Import ohne Daten, und keine rohe Fehlermeldung.
    expect(
      screen.queryByRole("button", { name: /Bring them over/i }),
    ).toBeNull();
    expect(mocks.runImportWithProgress).not.toHaveBeenCalled();
  });

  it("names the running source app as the thing to quit", async () => {
    mocks.scanImportSource.mockResolvedValue(
      scan({ problems: ["source_open"] }),
    );

    renderDialog();
    await chooseFolder();

    expect(
      await screen.findByText(
        /running right now and is holding its data open/i,
      ),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: /Look again/i })).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /Bring them over/i }),
    ).toBeNull();
    expect(mocks.runImportWithProgress).not.toHaveBeenCalled();
  });

  it("refuses to start when the disk is too small for the audio", async () => {
    mocks.scanImportSource.mockResolvedValue(
      scan({ audioBytes: 80 * GB, freeBytesOnTarget: 3 * GB }),
    );

    renderDialog();
    await chooseFolder();

    expect(await screen.findByText(/only 3.0 GB is free/i)).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /Bring them over/i }),
    ).toBeNull();
    expect(mocks.runImportWithProgress).not.toHaveBeenCalled();
  });

  it("lets the person past the space wall by leaving the audio behind", async () => {
    mocks.scanImportSource.mockResolvedValue(
      scan({
        audioBytes: 80 * GB,
        freeBytesOnTarget: 3 * GB,
        problems: ["not_enough_space"],
      }),
    );

    renderDialog();
    await chooseFolder();

    await screen.findByText(/only 3.0 GB is free/i);
    fireEvent.click(screen.getByRole("checkbox"));

    expect(
      await screen.findByRole("button", { name: /Bring them over/i }),
    ).toBeTruthy();
    // Der Probelauf muss die Ton-Entscheidung mittragen, sonst misst er etwas
    // anderes als der Import danach tut.
    await waitFor(() => {
      expect(mocks.runImportWithProgress).toHaveBeenCalledWith(
        expect.objectContaining({ dryRun: true, copyAudio: false }),
      );
    });
  });

  // Die Zeilensumme darf nicht als Gespraechszahl durchgehen: am gemessenen
  // Bestand stehen 2.355 Zeilen gegen 395 Gespraeche, Faktor sechs. Der Scan
  // direkt darueber sagte schon immer die richtige Zahl -- zwei Zahlen ueber
  // dieselbe Sache im selben Dialog.
  it("nennt Gespraeche und nicht die Zeilensumme", async () => {
    mocks.scanImportSource.mockResolvedValue(scan());

    renderDialog();
    await chooseFolder();

    const trial = await screen.findByTestId("anarlog-dry-run");
    expect(trial.textContent).toContain("42");
    expect(trial.textContent).not.toContain("252");
  });

  // Ein gescheiterter Trockenlauf sah aus wie ein geglueckter: die Oberflaeche
  // sah seinen Status nie an, und der Grund wurde nirgends gelesen.
  it("sagt beim gescheiterten Probelauf, woran es lag", async () => {
    mocks.scanImportSource.mockResolvedValue(scan());
    mocks.runImportWithProgress.mockResolvedValue(
      report({ status: "failed", error: "source_open" }),
    );

    renderDialog();
    await chooseFolder();

    const trial = await screen.findByTestId("anarlog-dry-run");
    expect(trial.textContent).toMatch(/is running right now/i);
    expect(trial.textContent).not.toMatch(/would come over/i);
  });

  it("shows the trial run before anything can be written", async () => {
    mocks.scanImportSource.mockResolvedValue(scan());

    renderDialog();
    await chooseFolder();

    const trial = await screen.findByTestId("anarlog-dry-run");
    expect(trial.textContent).toContain("42");
    expect(trial.textContent).toContain("Nothing has been changed yet");

    // Genau ein Lauf bisher, und der war trocken.
    expect(mocks.runImportWithProgress).toHaveBeenCalledTimes(1);
    expect(mocks.runImportWithProgress.mock.calls[0]?.[0]).toMatchObject({
      dryRun: true,
    });
    expect(
      screen.getByRole("button", { name: /Bring them over/i }),
    ).toBeTruthy();
  });

  it("writes only after the person confirms, then reports what arrived", async () => {
    mocks.scanImportSource.mockResolvedValue(scan());
    mocks.runImportWithProgress
      .mockResolvedValueOnce(report())
      .mockResolvedValueOnce(
        report({
          dryRun: false,
          imported: 240,
          conversationsImported: 40,
          skipped: 2,
          errors: 1,
          audioCopied: 20,
          audioBytesCopied: 2 * GB,
          status: "completed_with_issues",
        }),
      );

    renderDialog();
    await chooseFolder();

    fireEvent.click(
      await screen.findByRole("button", { name: /Bring them over/i }),
    );

    const final = await screen.findByTestId("anarlog-final-report");
    expect(final.textContent).toContain("40");
    expect(final.textContent).toContain("could not be brought over");
    expect(final.textContent).toContain("2.0 GB");
    expect(mocks.runImportWithProgress.mock.calls[1]?.[0]).toMatchObject({
      dryRun: false,
    });
    expect(screen.getByRole("button", { name: /Done/i })).toBeTruthy();
  });

  it("says the trial run did not get through instead of offering the import", async () => {
    mocks.scanImportSource.mockResolvedValue(scan());
    mocks.runImportWithProgress.mockRejectedValue(new Error("source vanished"));

    renderDialog();
    await chooseFolder();

    expect(
      await screen.findByText(/trial run did not get through/i),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /Bring them over/i }),
    ).toBeNull();
  });

  it("explains an unreadable folder rather than surfacing the raw failure", async () => {
    mocks.scanImportSource.mockRejectedValue(
      new Error("sqlite: unable to open database file (14)"),
    );

    renderDialog();
    await chooseFolder();

    expect(
      await screen.findByText(/This folder could not be read/i),
    ).toBeTruthy();
    expect(screen.queryByText(/sqlite/i)).toBeNull();
  });

  it("does nothing at all when the folder picker is dismissed", async () => {
    mocks.selectFolder.mockResolvedValue(null);

    renderDialog();
    await chooseFolder();

    await waitFor(() => {
      expect(mocks.selectFolder).toHaveBeenCalledOnce();
    });
    expect(mocks.scanImportSource).not.toHaveBeenCalled();
    expect(mocks.runImportWithProgress).not.toHaveBeenCalled();
  });
});
