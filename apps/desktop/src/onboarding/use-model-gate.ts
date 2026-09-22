// Verdrahtung fuer den Modell-Gate im Startblock.
//
// model-gate.ts ist rein und kennt weder React noch Tauri noch Einstellungen.
// Dieser Hook sammelt die unreine Welt ein (Download-Fortschritt, Cloud-Status,
// Einstellungen) und befragt damit die reine Logik. Alles, was hier an
// Zustand entsteht, ist Verdrahtung -- die Entscheidungen selbst fallen in
// model-gate.ts.

import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

import {
  canFinishOnboarding,
  getModelGateState,
  shouldStartModelDownload,
  type ModelGateState,
} from "./model-gate";

import { useConnectionHealth } from "~/settings/ai/stt/health";
import { useSetSettingValues } from "~/settings/queries";
import { useConfigValues } from "~/shared/config";
import { LOCAL_STT_DEFAULT_SELECTION } from "~/stt/model-selection";
import { localSttQueries, useLocalModelDownload } from "~/stt/useLocalSttModel";

/**
 * Die eingebauten LOKALEN Anbieter-Kennungen (siehe auch health.tsx,
 * isManagedProvider). Ein current_stt_provider, der hier nicht drinsteht und
 * trotzdem gesetzt ist, benennt einen Cloud-Anbieter.
 */
const BUILT_IN_LOCAL_STT_PROVIDER_IDS = new Set([
  "soniqo",
  "apple_speech",
  "whispercpp",
  "local_file",
]);

export function useOnboardingModelGate(): {
  state: ModelGateState;
  canFinish: boolean;
  sizeBytes: number | null;
  retry: () => void;
  retryCount: number;
} {
  // LOCAL_STT_DEFAULT_SELECTION.model ist als Literal "soniqo-parakeet-batch"
  // getippt, das ist bereits Teil der LocalModel-Union (SoniqoModel in
  // bindings.gen.ts) -- kein Cast noetig.
  const model = LOCAL_STT_DEFAULT_SELECTION.model;

  const {
    progress,
    errorMessage,
    isDownloaded,
    isDownloadedLoading,
    showProgress,
    handleDownload,
  } = useLocalModelDownload(model);

  const { current_stt_provider } = useConfigValues([
    "current_stt_provider",
  ] as const);
  const connectionHealth = useConnectionHealth();

  const cloudProviderReady =
    !!current_stt_provider &&
    !BUILT_IN_LOCAL_STT_PROVIDER_IDS.has(current_stt_provider) &&
    connectionHealth.status === "success";

  const [retryCount, setRetryCount] = useState(0);

  // showProgress deckt sowohl "gerade angestossen" (isStarting, bevor die
  // Hintergrund-Abfrage isDownloading.data nachzieht) als auch "laeuft im
  // Hintergrund" (isDownloading.data) ab -- genau das, was ModelGateInput.
  // isDownloading meint. Der blosse isDownloading.data-Wert aus der Abfrage
  // haette eine Luecke zwischen Klick und dem ersten Nachziehen der Abfrage.
  const gateInput = {
    isDownloaded,
    isDownloadedLoading,
    isDownloading: showProgress,
    progress,
    errorMessage,
    retryCount,
    cloudProviderReady,
  };

  const state = getModelGateState(gateInput);

  // Selbststart. Kein zusaetzlicher useRef-Merker fuer "schon gestartet"
  // noetig: shouldStartModelDownload sperrt bereits gegen jeden Zustand, der
  // einen zweiten Start unsauber machen wuerde (schon geladen, Abfrage noch
  // offen, laeuft schon, Fehler liegt an), UND handleDownload selbst sperrt
  // nochmal auf derselben Grundlage (isDownloaded.data || isDownloading.data
  // || isStarting, siehe useLocalSttModel.ts). Der Effekt darf also bei jedem
  // Render neu pruefen, ohne dass daraus ein zweiter Download entstehen kann.
  useEffect(() => {
    if (shouldStartModelDownload(gateInput)) {
      handleDownload();
    }
    // Deps bewusst ohne "progress": shouldStartModelDownload haengt nicht
    // davon ab, und progress aendert sich waehrend eines Downloads staendig
    // -- der Effekt wuerde sonst bei jedem Fortschritts-Tick neu pruefen.
  }, [
    isDownloaded,
    isDownloadedLoading,
    showProgress,
    errorMessage,
    retryCount,
    cloudProviderReady,
    handleDownload,
  ]);

  // Anbieter setzen, sobald das Modell liegt -- aber nur EINMAL und nur wenn
  // noch KEINER gesetzt ist. Eine bestehende Wahl (lokal oder Cloud) wird nie
  // ueberschrieben. Der Ref verhindert einen zweiten Schreibvorgang, solange
  // current_stt_provider noch nicht aus der Datenbank zurueckgekommen ist.
  const hasSetProviderRef = useRef(false);
  const setSelection = useSetSettingValues();
  useEffect(() => {
    if (isDownloaded && !current_stt_provider && !hasSetProviderRef.current) {
      hasSetProviderRef.current = true;
      setSelection({
        current_stt_provider: LOCAL_STT_DEFAULT_SELECTION.provider,
        current_stt_model: LOCAL_STT_DEFAULT_SELECTION.model,
      });
    }
  }, [isDownloaded, current_stt_provider, setSelection]);

  // handleDownload leert errorMessage synchron bei jedem Aufruf (die erste
  // Zeile in useLocalSttModel.ts' handleDownload ist setErrorMessage(null),
  // noch vor jedem await) -- ein Retry haengt deshalb nicht am alten Fehler.
  // retryCount hochzaehlen und handleDownload direkt rufen genuegt; ein
  // erneutes Befragen von shouldStartModelDownload ist hier nicht noetig,
  // weil ein Retry eine ausdrueckliche Nutzer-Handlung ist, keine Automatik.
  const retry = () => {
    setRetryCount((count) => count + 1);
    handleDownload();
  };

  const supportedModels = useQuery(localSttQueries.supportedModels());
  // model.key ist vom Typ LocalModel (eine Vereinigung von String-Literalen,
  // siehe bindings.gen.ts) -- Vergleich per === wie in select.tsx
  // (buildOnDeviceModelEntries: "model.key === recommendedModel").
  const sizeBytes =
    supportedModels.data?.find((entry) => entry.key === model)?.size_bytes ??
    null;

  return {
    state,
    canFinish: canFinishOnboarding(state),
    sizeBytes,
    retry,
    retryCount,
  };
}
