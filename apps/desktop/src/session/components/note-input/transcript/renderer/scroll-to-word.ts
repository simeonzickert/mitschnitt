// Feinjustierung des Suchsprungs im Transkript.
//
// Der Sprung zu einem Treffer scrollt zunaechst zum SEGMENT (scrollToIndex in
// virtual-segments.tsx). Das genuegte, solange Bloecke klein waren. Seit
// Bloecke desselben Sprechers zusammengelegt werden, kann ein Block hoeher
// sein als das Fenster -- dann rechnet scrollToIndex
// `max(0, (clientHeight - rowHeight) / 2)` zu 0 und landet am BLOCKANFANG,
// waehrend der Treffer weit darunter liegt und ungesehen bleibt.
//
// Die Feinjustierung in search/context.tsx greift hier nicht: sie sucht
// `[data-word-id]`, das Transkript setzt aber `data-transcript-word-id`
// (word-span.tsx).
//
// Zwei Zeitpunkte, die nicht verwechselt werden duerfen:
//
//   * Ob der Treffer SCHON sichtbar ist, wird VOR dem Segment-Scroll gemessen.
//     Danach waere die Messung wertlos, weil der weiche Scroll noch nicht
//     gelaufen ist und jedes Wort als unsichtbar gaelte.
//   * Das Nachjustieren wartet dagegen ueber mehrere Frames, weil die
//     Virtualisierung den Zielblock bei einem weiten Sprung erst rendern muss.
//     Ein einzelner Frame reicht dafuer nicht -- der Block existiert dann noch
//     gar nicht im DOM.

// Genug Frames, damit die Virtualisierung einen weit entfernten Block rendern
// kann, und wenig genug, dass ein aussichtsloser Fall nicht ewig nachhaengt.
const MAX_FRAMES = 30;

function escapeAttributeValue(value: string): string {
  if (typeof CSS !== "undefined" && typeof CSS.escape === "function") {
    return CSS.escape(value);
  }
  return value.replace(/["\\]/g, "\\$&");
}

function findeWort(
  container: HTMLElement | null,
  wordId: string | null | undefined,
): HTMLElement | null {
  if (!container || !wordId) return null;
  return container.querySelector<HTMLElement>(
    `[data-transcript-word-id="${escapeAttributeValue(wordId)}"]`,
  );
}

export function isWordVisible(
  containerRect: { top: number; bottom: number },
  wordRect: { top: number; bottom: number },
): boolean {
  return (
    wordRect.top >= containerRect.top && wordRect.bottom <= containerRect.bottom
  );
}

export function isWordCurrentlyVisible(
  container: HTMLElement | null,
  wordId: string | null | undefined,
): boolean {
  const element = findeWort(container, wordId);
  if (!element || !container) return false;
  return isWordVisible(
    container.getBoundingClientRect(),
    element.getBoundingClientRect(),
  );
}

// Wartet, bis die Virtualisierung das Wort gerendert hat, und scrollt dann
// dorthin. Gibt auf, statt endlos zu suchen.
export function scrollWordIntoView(
  container: HTMLElement | null,
  wordId: string | null | undefined,
  // Einschleusbar, damit der Test nicht auf echte Frames warten muss.
  requestFrame: (callback: () => void) => void = (callback) =>
    requestAnimationFrame(callback),
  maxFrames: number = MAX_FRAMES,
): void {
  if (!container || !wordId) return;

  let verbleibend = maxFrames;

  const versuch = () => {
    const element = findeWort(container, wordId);
    if (element) {
      element.scrollIntoView({ behavior: "smooth", block: "center" });
      return;
    }
    verbleibend -= 1;
    if (verbleibend > 0) {
      requestFrame(versuch);
    }
  };

  requestFrame(versuch);
}

// Der Sprung zu einem Suchtreffer, als eigene Einheit statt als Closure im
// Suchindex -- sonst waere die Verdrahtung nicht pruefbar.
export function createTranscriptJump({
  scrollToIndex,
  getContainer,
  index,
  wordId,
  requestFrame,
  maxFrames,
}: {
  scrollToIndex: (index: number, behavior: ScrollBehavior) => void;
  getContainer: () => HTMLElement | null;
  index: number;
  wordId: string | null | undefined;
  requestFrame?: (callback: () => void) => void;
  maxFrames?: number;
}): () => void {
  return () => {
    const container = getContainer();
    // Solange noch nichts gescrollt ist, ist diese Messung aussagekraeftig:
    // liegt der naechste Treffer bereits sichtbar vor uns (der haeufige Fall
    // beim Durchsteppen innerhalb eines Absatzes), bleibt die Ansicht ruhig.
    const bereitsSichtbar = isWordCurrentlyVisible(container, wordId);

    // Erst zum Block, damit die Virtualisierung ihn ueberhaupt rendert.
    scrollToIndex(index, "smooth");

    if (bereitsSichtbar) return;

    scrollWordIntoView(container, wordId, requestFrame, maxFrames);
  };
}
