import { describe, expect, it, vi } from "vitest";

import {
  createTranscriptJump,
  isWordCurrentlyVisible,
  isWordVisible,
  scrollWordIntoView,
} from "./scroll-to-word";

// Der Frame wird eingeschleust, damit der Test nicht auf einen echten wartet.
const sofort = (callback: () => void) => callback();

type WortLage = { id: string; top: number; bottom: number };

function baueContainer(
  containerRect: { top: number; bottom: number },
  woerter: WortLage[],
) {
  const scrollIntoView = vi.fn();
  // Veraenderbar, damit ein Test das spaetere Rendern der Virtualisierung
  // nachstellen kann.
  const vorhanden = new Map<string, WortLage>();
  for (const wort of woerter) vorhanden.set(wort.id, wort);

  const container = {
    getBoundingClientRect: () => containerRect,
    querySelector: (selektor: string) => {
      const treffer = /\[data-transcript-word-id="(.*)"\]/.exec(selektor);
      if (!treffer) return null;
      const id = treffer[1]!.replace(/\\/g, "");
      const wort = vorhanden.get(id);
      if (!wort) return null;
      return {
        getBoundingClientRect: () => ({ top: wort.top, bottom: wort.bottom }),
        scrollIntoView,
      } as unknown as HTMLElement;
    },
  } as unknown as HTMLElement;

  return {
    container,
    scrollIntoView,
    spaeterRendern: (wort: WortLage) => vorhanden.set(wort.id, wort),
  };
}

describe("isWordVisible", () => {
  it("nennt ein Wort innerhalb des Fensters sichtbar", () => {
    expect(
      isWordVisible({ top: 0, bottom: 500 }, { top: 100, bottom: 120 }),
    ).toBe(true);
  });

  it("nennt ein Wort unterhalb des Fensters unsichtbar", () => {
    expect(
      isWordVisible({ top: 0, bottom: 500 }, { top: 800, bottom: 820 }),
    ).toBe(false);
  });

  it("nennt ein Wort oberhalb des Fensters unsichtbar", () => {
    expect(
      isWordVisible({ top: 100, bottom: 500 }, { top: 20, bottom: 40 }),
    ).toBe(false);
  });

  it("nennt ein Wort, das unten herausragt, unsichtbar", () => {
    expect(
      isWordVisible({ top: 0, bottom: 500 }, { top: 480, bottom: 540 }),
    ).toBe(false);
  });
});

describe("isWordCurrentlyVisible", () => {
  it("meldet ein sichtbares Wort", () => {
    const { container } = baueContainer({ top: 0, bottom: 500 }, [
      { id: "a", top: 100, bottom: 120 },
    ]);
    expect(isWordCurrentlyVisible(container, "a")).toBe(true);
  });

  it("meldet ein nicht gerendertes Wort als nicht sichtbar", () => {
    const { container } = baueContainer({ top: 0, bottom: 500 }, []);
    expect(isWordCurrentlyVisible(container, "a")).toBe(false);
  });

  it("meldet ohne Container nicht sichtbar", () => {
    expect(isWordCurrentlyVisible(null, "a")).toBe(false);
  });
});

describe("scrollWordIntoView", () => {
  it("scrollt zum Treffer, sobald er gerendert ist", () => {
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [{ id: "wort-1", top: 1800, bottom: 1820 }],
    );

    scrollWordIntoView(container, "wort-1", sofort);

    expect(scrollIntoView).toHaveBeenCalledOnce();
    expect(scrollIntoView).toHaveBeenCalledWith({
      behavior: "smooth",
      block: "center",
    });
  });

  it("wartet ueber mehrere Frames, bis die Virtualisierung den Block rendert", () => {
    // Der Befund aus der Pruefung: bei einem weiten Sprung existiert der
    // Zielblock nach EINEM Frame noch nicht. Wer nur einmal nachsieht,
    // findet nichts und gibt still auf.
    const { container, scrollIntoView, spaeterRendern } = baueContainer(
      { top: 0, bottom: 500 },
      [],
    );
    const warteschlange: (() => void)[] = [];
    const einreihen = (callback: () => void) => warteschlange.push(callback);

    scrollWordIntoView(container, "wort-1", einreihen);

    // Drei Frames lang ist der Block noch nicht da.
    for (let i = 0; i < 3; i += 1) {
      warteschlange.shift()!();
      expect(scrollIntoView).not.toHaveBeenCalled();
    }

    spaeterRendern({ id: "wort-1", top: 1800, bottom: 1820 });
    warteschlange.shift()!();

    expect(scrollIntoView).toHaveBeenCalledOnce();
  });

  it("gibt nach dem Versuchslimit auf, statt endlos zu suchen", () => {
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [],
    );
    let frames = 0;
    const zaehlen = (callback: () => void) => {
      frames += 1;
      if (frames > 50) throw new Error("laeuft endlos");
      callback();
    };

    scrollWordIntoView(container, "wort-1", zaehlen, 5);

    expect(frames).toBe(5);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("bleibt still ohne Container", () => {
    const rahmen = vi.fn();
    scrollWordIntoView(null, "wort-1", rahmen);
    expect(rahmen).not.toHaveBeenCalled();
  });

  it("bleibt still ohne Wortkennung", () => {
    const rahmen = vi.fn();
    const { container } = baueContainer({ top: 0, bottom: 500 }, []);
    scrollWordIntoView(container, null, rahmen);
    expect(rahmen).not.toHaveBeenCalled();
  });

  it("sucht erst im naechsten Frame, nicht sofort", () => {
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [{ id: "wort-1", top: 1800, bottom: 1820 }],
    );
    const abgelegt: (() => void)[] = [];

    scrollWordIntoView(container, "wort-1", (callback) => {
      abgelegt.push(callback);
    });

    expect(abgelegt).toHaveLength(1);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });
});

describe("createTranscriptJump", () => {
  it("springt zum Block UND justiert danach zum Wort nach", () => {
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [{ id: "wort-7", top: 1800, bottom: 1820 }],
    );
    const scrollToIndex = vi.fn();

    createTranscriptJump({
      scrollToIndex,
      getContainer: () => container,
      index: 42,
      wordId: "wort-7",
      requestFrame: sofort,
    })();

    expect(scrollToIndex).toHaveBeenCalledWith(42, "smooth");
    expect(scrollIntoView).toHaveBeenCalledOnce();
  });

  it("misst die Sichtbarkeit VOR dem Segment-Scroll, nicht danach", () => {
    // Sonst waere der Waechter eine Attrappe: nach dem Absetzen eines weichen
    // Scrolls gilt einen Frame lang jedes Wort als unsichtbar.
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [{ id: "wort-7", top: 200, bottom: 220 }],
    );
    const reihenfolge: string[] = [];
    const scrollToIndex = vi.fn(() => {
      reihenfolge.push("segment-scroll");
    });

    createTranscriptJump({
      scrollToIndex,
      getContainer: () => container,
      index: 3,
      wordId: "wort-7",
      requestFrame: (callback) => {
        reihenfolge.push("frame");
        callback();
      },
    })();

    expect(scrollToIndex).toHaveBeenCalledWith(3, "smooth");
    // Sichtbar vor dem Scroll: die Ansicht wird nicht zusaetzlich bewegt.
    expect(scrollIntoView).not.toHaveBeenCalled();
    expect(reihenfolge).toEqual(["segment-scroll"]);
  });

  it("justiert nach, wenn der Treffer vor dem Sprung nicht sichtbar war", () => {
    const { container, scrollIntoView } = baueContainer(
      { top: 0, bottom: 500 },
      [{ id: "wort-7", top: 1800, bottom: 1820 }],
    );

    createTranscriptJump({
      scrollToIndex: vi.fn(),
      getContainer: () => container,
      index: 9,
      wordId: "wort-7",
      requestFrame: sofort,
    })();

    expect(scrollIntoView).toHaveBeenCalledOnce();
  });

  it("holt den Container erst beim Springen, nicht beim Bauen", () => {
    // Der Suchindex wird selten neu gebaut. Haenge der Container in der
    // Closure, zeigte er nach einem Neuaufbau der Ansicht ins Leere.
    const scrollToIndex = vi.fn();
    let container: HTMLElement | null = null;

    const springen = createTranscriptJump({
      scrollToIndex,
      getContainer: () => container,
      index: 1,
      wordId: "wort-1",
      requestFrame: sofort,
    });

    const gebaut = baueContainer({ top: 0, bottom: 500 }, [
      { id: "wort-1", top: 1800, bottom: 1820 },
    ]);
    container = gebaut.container;

    springen();

    expect(gebaut.scrollIntoView).toHaveBeenCalledOnce();
  });
});
