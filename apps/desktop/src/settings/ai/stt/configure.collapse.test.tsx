import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  current: "" as string,
  pending: null as string | null,
  configured: {} as Record<string, { configured: boolean }>,
}));

vi.mock("@lingui/react/macro", () => ({
  Trans: ({ children }: { children?: ReactNode }) => <>{children}</>,
  useLingui: () => ({
    t: (input: TemplateStringsArray | string, ...values: unknown[]) =>
      typeof input === "string"
        ? input
        : input.reduce(
            (acc, part, i) =>
              `${acc}${part}${i < values.length ? String(values[i]) : ""}`,
            "",
          ),
  }),
}));

vi.mock("~/shared/config", () => ({
  useConfigValue: () => mocks.current,
}));

vi.mock("./context", () => ({
  useSttSettings: () => ({
    accordionValue: "",
    setAccordionValue: () => {},
    pendingProvider: mocks.pending,
  }),
}));

vi.mock("./select", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./select")>()),
  useConfiguredMapping: () => ({ providers: mocks.configured, isReady: true }),
}));

vi.mock("~/settings/ai/shared", async (importOriginal) => ({
  ...(await importOriginal<typeof import("~/settings/ai/shared")>()),
  // Card internals are not under test; render only the provider id.
  NonAnarlogProviderCard: ({ config }: { config: { id: string } }) => (
    <div data-testid="card">{config.id}</div>
  ),
}));

import { ConfigureProviders } from "./configure";
import { STT_LOCAL_PROVIDER_IDS, STT_TOP_PROVIDER_IDS } from "./select";
import { PROVIDERS } from "./shared";

const MORE_IDS = PROVIDERS.filter(
  (p) =>
    !("builtIn" in p) &&
    !STT_LOCAL_PROVIDER_IDS.includes(p.id as never) &&
    !STT_TOP_PROVIDER_IDS.includes(p.id as never),
).map((p) => p.id as string);

const cards = () => screen.getAllByTestId("card").map((c) => c.textContent);

describe("STT ConfigureProviders: folded More group", () => {
  beforeEach(() => {
    mocks.current = "";
    mocks.pending = null;
    mocks.configured = {};
  });
  afterEach(cleanup);

  it("shows a 'More (N)' header and hides the untouched providers", () => {
    render(<ConfigureProviders />);
    expect(
      screen.getByRole("button", { name: `More (${MORE_IDS.length})` }),
    ).toBeTruthy();
    for (const id of cards()) {
      expect(MORE_IDS).not.toContain(id);
    }
  });

  it("opens the group on an active click", () => {
    render(<ConfigureProviders />);
    fireEvent.click(screen.getByRole("button", { name: /^More \(/ }));
    expect(cards()).toEqual(expect.arrayContaining(MORE_IDS));
  });

  it("keeps a pending (picked in the dropdown) provider visible", () => {
    mocks.pending = MORE_IDS[0];
    render(<ConfigureProviders />);
    expect(cards()).toContain(MORE_IDS[0]);
    expect(
      screen.getByRole("button", { name: `More (${MORE_IDS.length - 1})` }),
    ).toBeTruthy();
  });

  it("keeps the selected provider visible", () => {
    mocks.current = MORE_IDS[1];
    render(<ConfigureProviders />);
    expect(cards()).toContain(MORE_IDS[1]);
  });
});
