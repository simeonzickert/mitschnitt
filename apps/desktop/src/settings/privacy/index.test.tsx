import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  setSettingValues: vi.fn(),
  authenticate: vi.fn(),
  refreshAvailability: vi.fn(),
  lockApp: vi.fn(),
  available: true as boolean | null,
  authenticating: false,
  platform: "macos" as string,
  values: {
    lock_app: false,
  },
}));

vi.mock("@tauri-apps/plugin-os", () => ({
  platform: () => mocks.platform,
}));

vi.mock("~/settings/queries", () => ({
  useSetSettingValues: () => mocks.setSettingValues,
  useStoredSettingValuesQuery: () => ({
    data: {
      values: mocks.values,
      hasValues: new Set(["lock_app"]),
    },
    isLoading: false,
    error: null,
  }),
}));

vi.mock("~/lock/store", () => ({
  useAppLock: (selector: (state: typeof mocks) => unknown) =>
    selector({
      available: mocks.available,
      authenticating: mocks.authenticating,
      authenticate: mocks.authenticate,
      refreshAvailability: mocks.refreshAvailability,
      lockApp: mocks.lockApp,
    } as never),
}));

import { SettingsPrivacy } from ".";

describe("SettingsPrivacy", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.values.lock_app = false;
    mocks.available = true;
    mocks.authenticating = false;
    mocks.platform = "macos";
    mocks.authenticate.mockResolvedValue(true);
    mocks.refreshAvailability.mockResolvedValue(true);
  });

  afterEach(cleanup);

  // Die Einwilligungs-Schalter (PostHog, Sentry) sind mit ihren Empfaengern
  // ausgebaut; der Test bleibt als Wache gegen ein Wiederauftauchen.
  it("renders only the app lock control (telemetry switches removed)", () => {
    render(<SettingsPrivacy />);

    expect(screen.getByRole("switch", { name: "Lock app" })).toBeTruthy();
    expect(
      screen.queryByRole("switch", { name: "Share usage data (PostHog)" }),
    ).toBeNull();
    expect(screen.queryByRole("switch", { name: "Sentry" })).toBeNull();
    expect(mocks.setSettingValues).not.toHaveBeenCalled();
  });

  it("requires device authentication before locking the app", async () => {
    render(<SettingsPrivacy />);

    fireEvent.click(screen.getByRole("switch", { name: "Lock app" }));

    await waitFor(() => {
      expect(mocks.authenticate).toHaveBeenCalled();
      expect(mocks.setSettingValues).toHaveBeenCalledWith({ lock_app: true });
      expect(mocks.lockApp).toHaveBeenCalled();
    });
  });
});
