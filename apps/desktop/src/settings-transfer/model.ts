import {
  commands as localSttCommands,
  type LocalModel,
} from "@anlg/plugin-local-stt";

import { getStoredSettingValues } from "~/settings/queries";
import { isLocalFileSttModel, isOnDeviceSttModel } from "~/stt/capabilities";

/**
 * Mitschnitt-Fork (F14). After an import, does the chosen transcription model
 * actually exist on this machine?
 *
 * Four answers, not two. "Could not check" is kept apart from "not there",
 * because a failed check that reports "not there" would send the user off to
 * download a model they already have -- and a failed check that reports "fine"
 * is exactly the silent non-functioning this feature is supposed to prevent.
 *
 * "local-file" is its own answer, not a `missing` with a model attached:
 * `local_stt_model_path` is withheld from every bundle (see `keys.ts`), so
 * the imported setup always says "use my model file" while naming no file
 * that exists here. There is nothing to download for it -- offering a
 * download button would be the same silent-non-functioning failure this type
 * exists to rule out, just spelled differently.
 */
export type LocalModelState =
  | { state: "not-applicable" }
  | { state: "ready"; model: LocalModel }
  | { state: "missing"; model: LocalModel }
  | { state: "local-file" }
  | { state: "unknown"; reason: string };

export async function checkSelectedLocalModel(): Promise<LocalModelState> {
  const { values } = await getStoredSettingValues();
  const provider = values.current_stt_provider;
  const model = values.current_stt_model;

  if (isLocalFileSttModel(provider, model)) {
    return { state: "local-file" };
  }

  if (!isOnDeviceSttModel(provider, model)) {
    return { state: "not-applicable" };
  }

  const result = await localSttCommands.isModelDownloaded(model);
  if (result.status === "error") {
    return { state: "unknown", reason: result.error };
  }

  return result.data ? { state: "ready", model } : { state: "missing", model };
}
