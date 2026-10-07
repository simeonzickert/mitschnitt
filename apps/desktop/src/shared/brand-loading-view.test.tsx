import { cleanup, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { BrandLoadingView } from "./brand-loading-view";
import { MITSCHNITT_MARK_VIEW_BOX } from "./mitschnitt-mark";

describe("BrandLoadingView", () => {
  afterEach(cleanup);

  it("shows the Mitschnitt mark while loading", () => {
    render(<BrandLoadingView />);

    const status = screen.getByRole("status", { name: "Loading" });
    expect(status.querySelectorAll("svg")).toHaveLength(2);
    expect(screen.queryByText(/Updating your data/)).toBeNull();
  });

  it("shows optional loading detail", () => {
    render(
      <BrandLoadingView detail="Updating your data. This may take a few minutes." />,
    );

    expect(
      screen.getByText("Updating your data. This may take a few minutes."),
    ).toBeTruthy();
  });
});

// Das Startbild in index.html erscheint VOR dem React-Mount und ist der
// Zwilling dieser Ansicht. Bis 29.09.2026 trug es noch das anarlog-Zeichen,
// weil nur die React-Seite umgestellt war.
describe("boot splash in index.html", () => {
  const html = readFileSync(resolve(__dirname, "../../index.html"), "utf8");
  const splash = html.slice(
    html.indexOf('id="boot-splash"'),
    html.indexOf('class="boot-splash-detail"'),
  );

  it("uses the Mitschnitt mark for base and shimmer", () => {
    const boxes = splash.match(/viewBox="([^"]+)"/g) ?? [];
    expect(boxes).toEqual([
      `viewBox="${MITSCHNITT_MARK_VIEW_BOX}"`,
      `viewBox="${MITSCHNITT_MARK_VIEW_BOX}"`,
    ]);
    for (const x of ["224", "352", "480", "608", "736"]) {
      expect(splash.split(`<rect x="${x}"`).length - 1).toBe(2);
    }
  });

  it("contains no path-drawn foreign mark", () => {
    expect(splash).not.toContain("<path");
  });
});
