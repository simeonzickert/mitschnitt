// Die EINE Stelle, an der steht, welche Modelle der Startblock laedt.
//
// Der Startblock laedt kuenftig zwei Modelle nacheinander: zuerst das
// Transkriptionsmodell (STT), danach das Sprachmodell fuer
// Zusammenfassungen (LLM). Beide Konstanten stehen bewusst hier
// zusammen, damit ein Modellwechsel genau eine Zeile aendert -- nie
// eine Aufrufstelle im Startblock selbst.

import type { GgufLlmModel } from "@anlg/plugin-local-llm";

import { LOCAL_STT_DEFAULT_SELECTION } from "~/stt/model-selection";

/** Das Transkriptionsmodell, das der Startblock laedt. */
export const ONBOARDING_STT_MODEL = LOCAL_STT_DEFAULT_SELECTION.model;

/**
 * Das Sprachmodell fuer Zusammenfassungen, das der Startblock laedt.
 * Standard ist Gemma 3 4B Q4 (2,49 GB). Ein zweiter Kandidat wird gerade
 * gemessen; welcher gewinnt, aendert AUSSCHLIESSLICH diese Zeile.
 *
 * WICHTIG: diese Konstante laedt zwar eine echte GGUF-Datei auf die Platte
 * (ueber useLocalLlmDownload, use-llm-download.ts), aber im Fork fuehrt sie
 * NICHTS aus. LlmServer::start_with_model_path in
 * crates/local-llm-core/src/server.rs gibt seit Upstream-Commit
 * c964621329 ("Remove Cactus and reduce startup network calls",
 * 08.06.2026) IMMER den Fehler "Local LLM is not supported on this
 * platform" zurueck -- Cactus war die Engine hinter diesem Pfad und wurde
 * in genau diesem Commit ersatzlos entfernt, ohne dass lokale
 * Text-Generierung (Zusammenfassungen) hier upstream durch einen Nachfolger
 * ersetzt wurde. Ein heruntergeladenes Modell liegt also nutzlos auf der
 * Platte, bis local-llm-core an eine echte Engine angeschlossen ist. Diese
 * Konstante ist bewusst unverdrahtet: getOnboardingGateState /
 * canFinishOnboardingWithModels / shouldStartLlmDownload (model-gate.ts)
 * und useLocalLlmDownload existieren, aber der Startblock ruft sie
 * NIRGENDS auf (use-model-gate.ts kennt nur STT). Kein Automatismus darf
 * das aendern, solange server.rs der Stub oben ist -- sonst laedt der
 * Startblock 2,49 GB fuer eine Zusammenfassungs-Funktion, die danach
 * garantiert mit genau dieser Fehlermeldung scheitert.
 */
export const ONBOARDING_LLM_MODEL: GgufLlmModel = "Gemma3_4bQ4";
