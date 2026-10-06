import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openUrl: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
}));
vi.mock("@lingui/core/macro", () => ({
  t: (input: TemplateStringsArray) => input.join(""),
}));
vi.mock("@anlg/plugin-opener2", () => ({
  commands: { openUrl: mocks.openUrl },
}));
vi.mock("@anlg/ui/components/ui/toast", () => ({
  sonnerToast: { error: mocks.toastError },
}));

import { SystemAccountsHint } from "./system-accounts-hint";

describe("SystemAccountsHint", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("shows a toast when the opener returns an error result", async () => {
    mocks.openUrl.mockResolvedValue({ status: "error", error: "nope" });
    render(<SystemAccountsHint />);
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
  });

  it("shows a toast when the opener rejects", async () => {
    mocks.openUrl.mockRejectedValue(new Error("boom"));
    render(<SystemAccountsHint />);
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(mocks.toastError).toHaveBeenCalledTimes(1));
  });

  it("stays silent on success", async () => {
    mocks.openUrl.mockResolvedValue({ status: "ok", data: null });
    render(<SystemAccountsHint />);
    fireEvent.click(screen.getByRole("button"));
    await waitFor(() => expect(mocks.openUrl).toHaveBeenCalled());
    expect(mocks.toastError).not.toHaveBeenCalled();
  });
});
