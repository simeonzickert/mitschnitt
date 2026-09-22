import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@anlg/ui/components/ui/select";

import { ProviderListToggle } from "./provider-list-toggle";

// Minimale Pruefbuehne: ein echtes Radix-Select mit zwei "Anbietern" und dem
// Umschalter dazwischen -- nicht die echte Einstellungsseite, die zieht
// Datenbank und Tauri mit.
function TestSelect({ onValueChange }: { onValueChange: (value: string) => void }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <Select value="" onValueChange={onValueChange}>
      <SelectTrigger>
        <SelectValue placeholder="Select a provider" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="provider-a">Provider A</SelectItem>
        <SelectItem value="provider-b">Provider B</SelectItem>
        <SelectSeparator />
        <ProviderListToggle
          expanded={expanded}
          onToggle={() => setExpanded((value) => !value)}
        />
        {expanded ? (
          <SelectGroup>
            <SelectLabel>More providers</SelectLabel>
            <SelectItem value="provider-c">Provider C</SelectItem>
          </SelectGroup>
        ) : null}
      </SelectContent>
    </Select>
  );
}

// Wartet einen echten Makrotask ab -- Radix' Pfeiltasten-Navigation
// verschiebt den Fokus ueber setTimeout(fn), kein Promise-Microtask.
async function flushTimers() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function openSelect() {
  fireEvent.click(screen.getByRole("combobox"));
  await flushTimers();
}

function pressArrowDown() {
  const active = document.activeElement;
  if (!active) {
    throw new Error("no active element to dispatch ArrowDown from");
  }
  fireEvent.keyDown(active, { key: "ArrowDown" });
}

describe("ProviderListToggle", () => {
  beforeEach(() => {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    Element.prototype.scrollIntoView = vi.fn();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders as a real option reachable by the roving tabindex", async () => {
    const onValueChange = vi.fn();
    render(<TestSelect onValueChange={onValueChange} />);

    await openSelect();

    const toggle = screen.getByText("Show more providers").closest('[role="option"]');
    expect(toggle).not.toBeNull();
  });

  it("reaches the toggle with ArrowDown and highlights it", async () => {
    const onValueChange = vi.fn();
    render(<TestSelect onValueChange={onValueChange} />);

    await openSelect();

    // Zwei Anbieter liegen vor der Zeile: Provider A ist nach dem Oeffnen
    // schon fokussiert (Radix fokussiert das erste Item automatisch), zwei
    // ArrowDown reichen also bis zum Umschalter.
    pressArrowDown();
    await flushTimers();
    pressArrowDown();
    await flushTimers();

    const toggle = screen.getByText("Show more providers").closest('[role="option"]');
    expect(toggle).not.toBeNull();
    expect(document.activeElement).toBe(toggle);
    expect(toggle?.getAttribute("data-highlighted")).toBe("");
  });

  it("Enter expands the list without closing the menu or selecting a value", async () => {
    const onValueChange = vi.fn();
    render(<TestSelect onValueChange={onValueChange} />);

    await openSelect();
    pressArrowDown();
    await flushTimers();
    pressArrowDown();
    await flushTimers();

    const toggle = screen.getByText("Show more providers").closest('[role="option"]')!;
    fireEvent.keyDown(toggle, { key: "Enter" });

    expect(await screen.findByText("Show fewer providers")).toBeTruthy();
    expect(screen.getByText("Provider C")).toBeTruthy();
    expect(screen.getByRole("option", { name: "Provider A" })).toBeTruthy();
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it("Space toggles back without closing the menu or selecting a value", async () => {
    const onValueChange = vi.fn();
    render(<TestSelect onValueChange={onValueChange} />);

    await openSelect();
    pressArrowDown();
    await flushTimers();
    pressArrowDown();
    await flushTimers();

    const toggle = screen.getByText("Show more providers").closest('[role="option"]')!;
    fireEvent.keyDown(toggle, { key: "Enter" });
    const expandedToggle = await screen.findByText("Show fewer providers");

    fireEvent.keyDown(expandedToggle.closest('[role="option"]')!, { key: " " });

    expect(await screen.findByText("Show more providers")).toBeTruthy();
    expect(screen.queryByText("Provider C")).toBeNull();
    expect(screen.getByRole("option", { name: "Provider A" })).toBeTruthy();
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it("does not react to keys that are not Enter or Space", async () => {
    const onValueChange = vi.fn();
    render(<TestSelect onValueChange={onValueChange} />);

    await openSelect();
    pressArrowDown();
    await flushTimers();
    pressArrowDown();
    await flushTimers();

    const toggle = screen.getByText("Show more providers").closest('[role="option"]')!;
    fireEvent.keyDown(toggle, { key: "a" });

    expect(screen.getByText("Show more providers")).toBeTruthy();
    expect(screen.queryByText("Provider C")).toBeNull();
    expect(onValueChange).not.toHaveBeenCalled();
  });
});
