import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ModelGateState } from "./model-gate";

const mocks = vi.hoisted(() => ({
  createSession: vi.fn(),
  flushAutomaticRelaunch: vi.fn(),
  getOrCreateWelcomeSession: vi.fn(),
  setOnboardingNeeded: vi.fn(),
  setPendingWelcomeSession: vi.fn(),
  // Ready/model + canFinish:true als Default, damit die bestehenden vier
  // Tests weiter einen freigegebenen "Open Mitschnitt"-Knopf sehen -- so wie
  // vor dem Modell-Gate. Einzelne Tests koennen das per
  // mocks.useOnboardingModelGate.mockReturnValue(...) ueberschreiben.
  // Rueckgabetyp ausdruecklich breit halten: ohne die Annotation leitet
  // vitest ihn aus dem Standardwert ab (ready/model, sizeBytes null), und
  // jeder Test, der einen ANDEREN Zustand einsetzt, scheitert an tsc.
  useOnboardingModelGate: vi.fn(
    (): {
      state: ModelGateState;
      canFinish: boolean;
      sizeBytes: number | null;
      retry: () => void;
      retryCount: number;
    } => ({
      state: { kind: "ready", reason: "model" },
      canFinish: true,
      sizeBytes: null,
      retry: vi.fn(),
      retryCount: 0,
    }),
  ),
}));

vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openUrl: vi.fn() },
}));

vi.mock("./welcome-note", () => ({
  getOrCreateWelcomeSession: mocks.getOrCreateWelcomeSession,
  setPendingWelcomeSession: mocks.setPendingWelcomeSession,
}));

// Der Modell-Gate-Hook zieht Tauri-Kommandos, react-query und die
// Einstellungen-Datenbank mit (siehe use-model-gate.ts). Fuer diese Datei
// zaehlt nur das Verhalten von FinalSection selbst -- die Zustandslogik ist
// bereits in model-gate.test.ts rein getestet -- deshalb wird der ganze Hook
// gemockt statt seine Abhaengigkeiten einzeln nachzubauen.
vi.mock("./use-model-gate", () => ({
  useOnboardingModelGate: mocks.useOnboardingModelGate,
}));

// STT ist die vollstaendige Transkriptions-Einstellungsseite (eigener
// Provider, eigene Tauri-Abfragen). Sie wird in FinalSection nur gerendert,
// wenn der Nutzer den Cloud-Umschalter aufklappt -- in keinem der vier Tests
// hier der Fall -- aber der Import muesste sonst ihren ganzen Baum laden.
vi.mock("~/settings/ai/stt", () => ({
  STT: () => null,
}));

vi.mock("~/session/queries", () => ({
  createSession: mocks.createSession,
}));

vi.mock("~/shared/relaunch", () => ({
  flushAutomaticRelaunch: mocks.flushAutomaticRelaunch,
}));

vi.mock("~/types/tauri.gen", () => ({
  commands: { setOnboardingNeeded: mocks.setOnboardingNeeded },
}));

import { FinalSection, finishOnboarding } from "./final";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.flushAutomaticRelaunch.mockResolvedValue(false);
  mocks.getOrCreateWelcomeSession.mockResolvedValue("welcome-session");
  mocks.setOnboardingNeeded.mockResolvedValue({ status: "ok", data: null });
  mocks.useOnboardingModelGate.mockReturnValue({
    state: { kind: "ready" as const, reason: "model" as const },
    canFinish: true,
    sizeBytes: null,
    retry: vi.fn(),
    retryCount: 0,
  });
});

afterEach(cleanup);

it("keeps the button locked while the model is still downloading", () => {
  const onContinue = vi.fn();
  mocks.useOnboardingModelGate.mockReturnValue({
    state: { kind: "downloading" as const, progress: 42 },
    canFinish: false,
    sizeBytes: 662_700_032,
    retry: vi.fn(),
    retryCount: 0,
  });

  render(<FinalSection onContinue={onContinue} />);

  const button = screen.getByRole("button", { name: "Open Mitschnitt" });
  expect(button).toHaveProperty("disabled", true);

  // Auch ein erzwungener Klick darf den Startblock nicht abschliessen.
  fireEvent.click(button);

  expect(mocks.setOnboardingNeeded).not.toHaveBeenCalled();
  expect(onContinue).not.toHaveBeenCalled();
});

it("offers the cloud provider way out while the button is locked", () => {
  mocks.useOnboardingModelGate.mockReturnValue({
    state: { kind: "error" as const, message: "no network", retryCount: 3, exhausted: true },
    canFinish: false,
    sizeBytes: null,
    retry: vi.fn(),
    retryCount: 3,
  });

  render(<FinalSection onContinue={vi.fn()} />);

  expect(
    screen.getByRole("button", { name: "I'd rather use a cloud provider" }),
  ).toBeTruthy();
  expect(screen.getByRole("button", { name: "Try again" })).toBeTruthy();
});

it("unlocks the button once a cloud provider is set up", () => {
  mocks.useOnboardingModelGate.mockReturnValue({
    state: { kind: "ready" as const, reason: "cloud" as const },
    canFinish: true,
    sizeBytes: null,
    retry: vi.fn(),
    retryCount: 0,
  });

  render(<FinalSection onContinue={vi.fn()} />);

  expect(
    screen.getByRole("button", { name: "Open Mitschnitt" }),
  ).toHaveProperty("disabled", false);
});

it("opens a blank note when welcome-note creation fails", async () => {
  const onContinue = vi.fn();
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  mocks.getOrCreateWelcomeSession.mockRejectedValueOnce(
    new Error("malformed JSON"),
  );
  mocks.createSession.mockResolvedValueOnce("blank-session");

  await finishOnboarding(onContinue);

  expect(mocks.createSession).toHaveBeenCalledTimes(1);
  expect(onContinue).toHaveBeenCalledWith("blank-session");
  consoleError.mockRestore();
});

it("shows a retryable error when onboarding cannot be persisted", async () => {
  const onContinue = vi.fn();
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  mocks.setOnboardingNeeded.mockResolvedValueOnce({
    status: "error",
    error: "settings unavailable",
  });

  render(<FinalSection onContinue={onContinue} />);
  fireEvent.click(screen.getByRole("button", { name: "Open Mitschnitt" }));

  expect(
    (
      screen.getByRole("button", {
        name: "Open Mitschnitt",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  await waitFor(() => {
    expect(screen.getByRole("alert").textContent).toBe(
      "Couldn't open Mitschnitt. Please try again.",
    );
  });
  expect(
    (
      screen.getByRole("button", {
        name: "Open Mitschnitt",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
  expect(onContinue).not.toHaveBeenCalled();
  consoleError.mockRestore();
});

it("reuses the blank fallback session when persistence is retried", async () => {
  const onContinue = vi.fn();
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  mocks.getOrCreateWelcomeSession.mockRejectedValue(
    new Error("malformed JSON"),
  );
  mocks.createSession.mockResolvedValue("blank-session");
  mocks.setOnboardingNeeded
    .mockResolvedValueOnce({ status: "error", error: "settings unavailable" })
    .mockResolvedValueOnce({ status: "ok", data: null });

  render(<FinalSection onContinue={onContinue} />);
  fireEvent.click(screen.getByRole("button", { name: "Open Mitschnitt" }));
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: "Open Mitschnitt" }));

  await waitFor(() => {
    expect(onContinue).toHaveBeenCalledWith("blank-session");
  });
  expect(mocks.createSession).toHaveBeenCalledTimes(1);
  consoleError.mockRestore();
});

it("ignores concurrent finish attempts", async () => {
  const onContinue = vi.fn();
  let resolveWelcomeSession: (sessionId: string) => void = () => {};
  mocks.getOrCreateWelcomeSession.mockReturnValue(
    new Promise((resolve) => {
      resolveWelcomeSession = resolve;
    }),
  );

  render(<FinalSection onContinue={onContinue} />);
  const button = screen.getByRole("button", { name: "Open Mitschnitt" });
  fireEvent.click(button);
  fireEvent.click(button);
  resolveWelcomeSession("welcome-session");

  await waitFor(() => {
    expect(onContinue).toHaveBeenCalledWith("welcome-session");
  });
  expect(mocks.getOrCreateWelcomeSession).toHaveBeenCalledTimes(1);
  expect(onContinue).toHaveBeenCalledTimes(1);
});
