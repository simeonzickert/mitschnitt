import type { TaskArgsMap, TaskArgsMapTransformed, TaskConfig } from ".";

import { loadSessionContentSnapshot } from "~/session/content-queries";
import { getParticipants, getSessionData } from "~/session/prompt-context";
import type { SettingValues } from "~/settings/schema";
import { parseDictionaryTermsJson } from "~/stt/keywords";

export const titleTransform: Pick<TaskConfig<"title">, "transformArgs"> = {
  transformArgs,
};

async function transformArgs(
  args: TaskArgsMap["title"],
  settingsValues: SettingValues,
): Promise<TaskArgsMapTransformed["title"]> {
  // Der Schnappschuss wird jetzt IMMER geladen, auch wenn die Notiz schon
  // mitgeliefert wurde: aus ihm kommt der Kontext (Kalendertitel, Zeitraum,
  // Teilnehmer), den der Titel-Prompt bis zum 03.09.2026 gar nicht hatte.
  // Fehlt er, bleibt der Titel moeglich -- nur ohne Kontext.
  const snapshot = await loadSessionContentSnapshot(args.sessionId);
  if (!args.enhancedNote && !snapshot) {
    throw new Error(`Session ${args.sessionId} no longer exists`);
  }

  const enhancedNote =
    args.enhancedNote ??
    snapshot?.enhancedNotes
      .map((note) => note.markdown)
      .filter(Boolean)
      .join("\n\n") ??
    "";
  const language = getLanguage(settingsValues);
  return {
    language,
    enhancedNote,
    session: snapshot ? getSessionData(snapshot) : null,
    participants: snapshot ? getParticipants(snapshot) : [],
    dictionaryTerms: parseDictionaryTermsJson(
      settingsValues.personalization_dictionary_terms,
    ),
  };
}

function getLanguage(settingsValues: SettingValues): string | null {
  const value = settingsValues.ai_language;
  return typeof value === "string" && value.length > 0 ? value : null;
}
