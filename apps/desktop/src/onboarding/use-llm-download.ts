// Download-Haken fuer das lokale Sprachmodell (LLM) im Startblock.
//
// Gegenstueck zu useLocalSttModel.ts's useLocalModelDownload, aber NICHT
// wiederverwendbar dafuer: das lokale-LLM-Plugin hat eine andere
// Fortschritts-Mechanik. local-stt sendet Fortschritt ueber einen globalen
// Tauri-Event-Strom (localSttEvents.downloadProgressPayload), den JEDER
// Beobachter hoert, unabhaengig davon wer den Download angestossen hat.
// local-llm hat KEINEN solchen Strom -- downloadModel(model, channel) nimmt
// stattdessen einen Kanal entgegen, den der Aufrufer selbst anlegt
// (@tauri-apps/api/core, Channel<number>, siehe plugins/db/js/index.ts fuer
// das gleiche Muster in diesem Repo). Nur WIR bekommen Fortschritt fuer
// Downloads, die WIR selbst ueber diesen Haken angestossen haben.
//
// Praktische Folge, die ein spaeterer Bite kennen muss: laeuft ein Download
// bereits im Hintergrund, bevor diese Komponente montiert wurde (z. B. ein
// zweiter Start waehrend ein frueherer Lauf noch nicht fertig ist), zeigt
// isDownloading zwar korrekt true (aus der Hintergrund-Abfrage), aber
// progress bleibt bei 0 -- es gibt keinen Kanal, an den sich nachtraeglich
// andocken liesse. Fuer den Startblock (genau ein Aufrufer je Modell) ist
// das kein Problem, fuer eine zweite Anzeigestelle desselben Downloads waere
// es eines.

import { queryOptions, useQuery } from "@tanstack/react-query";
import { useCallback, useState } from "react";

import { Channel } from "@tauri-apps/api/core";

import {
  commands as localLlmCommands,
  type GgufLlmModel,
} from "@anlg/plugin-local-llm";

export const localLlmKeys = {
  all: ["local-llm"] as const,
  models: () => [...localLlmKeys.all, "model"] as const,
  model: (model: GgufLlmModel) => [...localLlmKeys.models(), model] as const,
  modelDownloaded: (model: GgufLlmModel) =>
    [...localLlmKeys.model(model), "downloaded"] as const,
  modelDownloading: (model: GgufLlmModel) =>
    [...localLlmKeys.model(model), "downloading"] as const,
};

export const localLlmQueries = {
  isDownloaded: (model: GgufLlmModel) =>
    queryOptions({
      queryKey: localLlmKeys.modelDownloaded(model),
      queryFn: () => localLlmCommands.isModelDownloaded(model),
      select: (result) => {
        if (result.status === "error") {
          throw new Error(result.error);
        }
        return result.data;
      },
    }),
  isDownloading: (model: GgufLlmModel) =>
    queryOptions({
      queryKey: localLlmKeys.modelDownloading(model),
      queryFn: () => localLlmCommands.isModelDownloading(model),
      select: (result) => {
        if (result.status === "error") {
          throw new Error(result.error);
        }
        return result.data;
      },
    }),
};

function clampProgress(value: number): number {
  if (value < 0) return 0;
  if (value > 100) return 100;
  return value;
}

/**
 * Form ist absichtlich deckungsgleich mit useLocalModelDownload's Rueckgabe
 * (siehe useLocalSttModel.ts), damit ein Aufrufer STT- und LLM-Download
 * ueber dieselbe Ansicht behandeln kann. Feldname "isDownloading" statt
 * STT's "showProgress": so passt das Feld ohne Umbenennen direkt in
 * ModelGateInput.isDownloading (model-gate.ts).
 */
export function useLocalLlmDownload(model: GgufLlmModel) {
  const [progress, setProgress] = useState(0);
  const [isStarting, setIsStarting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const isDownloadedQuery = useQuery(localLlmQueries.isDownloaded(model));
  const isDownloadingQuery = useQuery(localLlmQueries.isDownloading(model));
  const refetchDownloaded = isDownloadedQuery.refetch;
  const refetchDownloading = isDownloadingQuery.refetch;

  const isDownloaded = isDownloadedQuery.data ?? false;
  // Wie STT's showProgress: deckt "gerade angestossen" (isStarting, bevor
  // die Hintergrund-Abfrage nachzieht) UND "laeuft im Hintergrund"
  // (isDownloadingQuery.data) ab. Das !isDownloaded-Gate verhindert, dass
  // isDownloading nach einem erfolgreichen Abschluss haengen bleibt, solange
  // die Downloading-Abfrage noch nicht auf false zurueckgezogen ist.
  const isDownloading =
    !isDownloaded && (isStarting || (isDownloadingQuery.data ?? false));

  const handleDownload = useCallback(() => {
    // Selbst sperren, genau wie useLocalSttModel.ts's handleDownload: schon
    // geladen oder schon am Laden darf keinen zweiten Download anstossen.
    if (isDownloadedQuery.data || isDownloadingQuery.data || isStarting) {
      return;
    }

    // Erste Zeile, synchron, VOR jedem await -- darauf verlaesst sich der
    // Retry-Weg im Modell-Gate: ein Fehler von einem frueheren Versuch darf
    // nicht stehen bleiben, sobald ein neuer Versuch beginnt.
    setErrorMessage(null);
    setIsStarting(true);
    setProgress(0);

    const channel = new Channel<number>();
    channel.onmessage = (value) => {
      // Negative Werte sind kein Prozent-Fortschritt. Die Rust-Seite
      // (plugins/local-llm/src/ext.rs, TauriModelRuntime::emit_progress
      // + der Verdraengungs-Pfad in LocalLlmExt::download_model) schickt -1
      // sowohl bei einem echten Fehlschlag als auch wenn dieser Kanal durch
      // einen ZWEITEN download_model-Aufruf fuer dasselbe Modell verdraengt
      // wurde. Der eigentliche Fehlertext kommt in beiden Faellen ueber das
      // Promise unten (result.status === "error") an -- hier nur
      // ignorieren, sonst faellt der Balken fuer einen Frame auf 0% zurueck
      // und sieht wie ein sauberer Neustart aus statt wie ein Fehler.
      if (value < 0) {
        return;
      }
      setProgress(clampProgress(value));
    };

    void localLlmCommands.downloadModel(model, channel).then((result) => {
      if (result.status === "error") {
        setErrorMessage(result.error);
        setIsStarting(false);
        setProgress(0);
        void refetchDownloading();
        return;
      }

      // Erfolgsfall: der Kanal hat vorher bereits die 100 gesendet (Rust
      // schickt DownloadStatus::Completed synchron, bevor download_model
      // zurueckkehrt), diese Zeilen sind das Sicherheitsnetz falls Kanal-
      // Nachricht und Promise-Aufloesung je auseinanderlaufen.
      setErrorMessage(null);
      setProgress(100);
      setIsStarting(false);
      void refetchDownloaded();
      void refetchDownloading();
    });
  }, [
    isDownloadedQuery.data,
    isDownloadingQuery.data,
    isStarting,
    model,
    refetchDownloaded,
    refetchDownloading,
  ]);

  const handleCancel = useCallback(() => {
    void localLlmCommands
      .cancelDownload(model)
      .then(() => refetchDownloading());
    setIsStarting(false);
    setProgress(0);
  }, [model, refetchDownloading]);

  return {
    progress,
    hasError: errorMessage !== null,
    errorMessage,
    isDownloaded,
    isDownloadedLoading: isDownloadedQuery.isLoading,
    isDownloading,
    handleDownload,
    handleCancel,
  };
}
