// Entscheidungslogik fuer den Modell-Gate im Startblock.
//
// Ausgangslage: ein Erstnutzer verlaesst den Startblock heute mit einer App,
// die aufnimmt, aber nichts transkribieren kann, weil kein Modell geladen
// wurde. Kuenftig laedt der Startblock das lokale Standardmodell
// (soniqo-parakeet-batch, 632 MiB) automatisch, und der Knopf "Open
// Mitschnitt" bleibt gesperrt, bis entweder das Modell liegt oder ein
// Cloud-Anbieter eingerichtet und geprueft ist.
//
// Dieses Modul ist rein: nur Ein- und Ausgabe, keine Seiteneffekte, kein
// Zugriff auf React, Tauri-Kommandos oder Einstellungen. Die Verdrahtung an
// den bestehenden Download-Haken macht ein anderer Bauer.

export type ModelGateInput = {
  /** Modell liegt schon auf der Platte (Zweitstart, Import, oder in den Einstellungen geladen). */
  isDownloaded: boolean;
  /** Die Abfrage "liegt es?" laeuft noch. Wichtig: vorher darf NICHTS angestossen werden. */
  isDownloadedLoading: boolean;
  /** Ein Download laeuft gerade (Start angestossen oder vom Hintergrund gemeldet). */
  isDownloading: boolean;
  /** 0 bis 100. */
  progress: number;
  /** Letzte Fehlermeldung des Downloads, sonst null. */
  errorMessage: string | null;
  /** Wie oft der Nutzer "Retry" gedrueckt hat. */
  retryCount: number;
  /** Ein Cloud-Anbieter fuer Transkription ist eingetragen UND seine Verbindung wurde geprueft. */
  cloudProviderReady: boolean;
};

export type ModelGateState =
  | { kind: "checking" }
  | { kind: "idle" }
  | { kind: "downloading"; progress: number }
  | { kind: "ready"; reason: "model" | "cloud" }
  | { kind: "error"; message: string; retryCount: number; exhausted: boolean };

export const MODEL_GATE_MAX_RETRIES = 3;

/**
 * Ermittelt den Zustand des Modell-Gates aus dem aktuellen Eingabe-Schnappschuss.
 *
 * Die Reihenfolge der Pruefungen ist der ganze Punkt dieser Funktion und darf
 * nicht umgestellt werden:
 *
 * 1. cloudProviderReady gewinnt gegen ALLES — auch gegen einen laufenden
 *    Download und gegen einen anliegenden Fehler. Wer einen Cloud-Anbieter
 *    eingerichtet hat, soll nicht auf ein lokales Modell warten muessen, das
 *    er gar nicht braucht.
 * 2. isDownloaded: das lokale Modell liegt bereits, damit ist der Nutzer
 *    fertig, unabhaengig davon was sonst noch an Zustand vorliegt (z. B. ein
 *    veralteter Fehler von einem frueheren Versuch).
 * 3. errorMessage: ein Fehler wird angezeigt, sobald keiner der beiden
 *    Ausgaenge oben getroffen hat.
 * 4. isDownloading: ein laufender Download zeigt Fortschritt.
 * 5. isDownloadedLoading: die Abfrage "liegt es schon?" laeuft noch, wir
 *    wissen also gar nicht, in welchem der obigen Zustaende wir eigentlich
 *    sind.
 * 6. Sonst: idle. Nichts liegt vor, nichts laeuft, kein Fehler — der Aufrufer
 *    darf jetzt einen Download anstossen (siehe shouldStartModelDownload).
 */
export function getModelGateState(input: ModelGateInput): ModelGateState {
  if (input.cloudProviderReady) {
    return { kind: "ready", reason: "cloud" };
  }

  if (input.isDownloaded) {
    return { kind: "ready", reason: "model" };
  }

  if (input.errorMessage !== null) {
    return {
      kind: "error",
      message: input.errorMessage,
      retryCount: input.retryCount,
      exhausted: input.retryCount >= MODEL_GATE_MAX_RETRIES,
    };
  }

  if (input.isDownloading) {
    return { kind: "downloading", progress: clampProgress(input.progress) };
  }

  if (input.isDownloadedLoading) {
    return { kind: "checking" };
  }

  return { kind: "idle" };
}

function clampProgress(progress: number): number {
  if (progress < 0) return 0;
  if (progress > 100) return 100;
  return progress;
}

/**
 * Der Onboarding-Knopf "Open Mitschnitt" darf nur bei kind === "ready"
 * freigegeben werden.
 *
 * Ausdruecklich auch NICHT bei einem erschoepften Retry (exhausted: true):
 * ein erschoepfter Retry sperrt den Nutzer nicht dauerhaft ein, aber der
 * vorgesehene Ausweg ist der Cloud-Weg (Regel 1 oben), nicht ein
 * aufgeweichtes Gate, das nach drei Fehlversuchen einfach durchwinkt. Diese
 * Zeile bewusst NICHT "reparieren", falls spaeter jemand denkt, exhausted
 * muesse den Knopf freigeben — das war eine getroffene Entscheidung, kein
 * Versehen.
 */
export function canFinishOnboarding(state: ModelGateState): boolean {
  return state.kind === "ready";
}

/**
 * True nur wenn ALLE Bedingungen zutreffen:
 * - kein Cloud-Anbieter eingerichtet (der braeuchte gar kein lokales Modell)
 * - Modell liegt noch nicht
 * - die Abfrage "liegt es?" ist NICHT mehr am Laufen
 * - kein Download laeuft bereits
 * - kein Fehler liegt an
 *
 * isDownloadedLoading ist bewusst mit in der Bedingung, obwohl es kein
 * eigener "kind" im Ergebnis von getModelGateState waere, der einen Download
 * verbieten muesste: ohne diese Klausel wuerde beim allerersten Rendern ein
 * Download gestartet, bevor ueberhaupt feststeht, ob das Modell schon auf
 * der Platte liegt. Genau das waere der Doppel-Download, den wir vermeiden
 * wollen (Zweitstart, Import, oder bereits in den Einstellungen geladen).
 *
 * Ein anliegender Fehler stoppt den automatischen Start ebenfalls: ein
 * Neuversuch ist eine ausdrueckliche Handlung des Nutzers. Der Aufrufer
 * erhoeht dabei retryCount und leert errorMessage, bevor er diese Funktion
 * erneut befragt.
 */
export function shouldStartModelDownload(input: ModelGateInput): boolean {
  return (
    !input.cloudProviderReady &&
    !input.isDownloaded &&
    !input.isDownloadedLoading &&
    !input.isDownloading &&
    input.errorMessage === null
  );
}

/**
 * Zustand des Startblocks fuer ZWEI Modelle: erst Transkription (STT), dann
 * das Sprachmodell fuer Zusammenfassungen (LLM). Die drei Funktionen oben
 * (getModelGateState, canFinishOnboarding, shouldStartModelDownload)
 * beschreiben EIN Modell und bleiben unveraendert -- sie werden fuer beide
 * benutzt, hier nur zu einem gemeinsamen Zustand zusammengefuehrt.
 */
export type OnboardingGateState = {
  stt: ModelGateState;
  llm: ModelGateState;
  /** Welches Modell gerade an der Reihe ist, oder null wenn beide fertig sind. */
  active: "stt" | "llm" | null;
};

/**
 * "An der Reihe" heisst hier ausdruecklich nicht "laedt gerade", sondern
 * "noch nicht ready" -- ein Modell im Fehlerzustand ist ebenfalls "active",
 * damit die Oberflaeche weiss, wessen Fehler/Retry-Knopf sie zeigen muss.
 */
export function getOnboardingGateState(
  stt: ModelGateInput,
  llm: ModelGateInput,
): OnboardingGateState {
  const sttState = getModelGateState(stt);
  const llmState = getModelGateState(llm);

  const active: "stt" | "llm" | null =
    sttState.kind !== "ready" ? "stt" : llmState.kind !== "ready" ? "llm" : null;

  return { stt: sttState, llm: llmState, active };
}

/**
 * Der Onboarding-Knopf "Open Mitschnitt" darf erst frei sein, wenn BEIDE
 * Modelle ready sind. Ein Cloud-Anbieter steckt bereits in der jeweiligen
 * ModelGateInput-Eingabe (cloudProviderReady) -- wer also fuer Transkription
 * UND Zusammenfassung je einen Cloud-Anbieter eingerichtet hat, ist hier
 * ready/ready und kommt genauso durch, ohne dass diese Funktion eine zweite
 * Cloud-Sonderregel bräuchte. Diese Zeile bewusst NICHT um einen eigenen
 * Cloud-Kurzschluss ergaenzen, falls spaeter jemand denkt, das fehle -- die
 * Abkuerzung loest sich bereits in getModelGateState auf.
 */
export function canFinishOnboardingWithModels(
  state: OnboardingGateState,
): boolean {
  return state.stt.kind === "ready" && state.llm.kind === "ready";
}

/** Duenner benannter Durchgriff, damit die Aufrufseite lesbar bleibt. */
export function shouldStartSttDownload(stt: ModelGateInput): boolean {
  return shouldStartModelDownload(stt);
}

/**
 * Erzwingt die Reihenfolge: das Sprachmodell darf erst starten, wenn die
 * Transkription bereits ready ist (STT-Zustand "ready", egal ob ueber
 * lokales Modell oder Cloud-Anbieter). Nacheinander statt gleichzeitig, weil
 * beide Downloads ueber dieselbe Leitung laufen wuerden (632 MiB STT plus
 * 2,49 GB LLM) -- getrennt ist der Fortschritt fuer den Nutzer lesbar (ein
 * Balken statt zwei, die sich gegenseitig die Bandbreite stehlen) und die
 * Fehlersuche einfacher (ein fehlgeschlagener Download je Zeitpunkt, nicht
 * zwei ueberlappende).
 */
export function shouldStartLlmDownload(
  stt: ModelGateInput,
  llm: ModelGateInput,
): boolean {
  return getModelGateState(stt).kind === "ready" && shouldStartModelDownload(llm);
}
