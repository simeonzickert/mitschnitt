import { useQuery } from "@tanstack/react-query";
import { arch, platform } from "@tauri-apps/plugin-os";
import { useMemo } from "react";

import { commands as localSttCommands } from "@anlg/plugin-local-stt";
import type { AIProviderStorage } from "@anlg/store";

import { type ProviderId } from "~/settings/ai/stt/shared";
import { useAiProvider } from "~/settings/providers";
import { useConfigValues } from "~/shared/config";
import {
  isLocalFileSttModel,
  isOnDeviceSttModel,
  isRealtimeLocalModel,
  resolveServableSttSelection,
} from "~/stt/capabilities";
import { localSttQueries } from "~/stt/useLocalSttModel";

export const useSTTConnection = () => {
  const stored = useConfigValues([
    "current_stt_provider",
    "current_stt_model",
    "local_stt_model_path",
  ] as const);
  // Fork (Grok 5b, Review 02.09.2026): this is the edge where the stored
  // selection becomes the connection for live and batch. A pair under the
  // original's hosted ids ("anarlog", "hyprnote") is normalised here as well,
  // not only by the start-up migration -- so it never reaches the `removed:`
  // arm in Rust, not even between app start and that migration. D5: and a
  // local pair on a platform without local transcription is not set here
  // either -- same function as the migration, so the two cannot disagree.
  const { provider: current_stt_provider, model: current_stt_model } =
    resolveServableSttSelection(
      stored.current_stt_provider,
      stored.current_stt_model,
      platform(),
      arch(),
    ) as {
      provider: ProviderId | undefined;
      model: string | undefined;
    };
  const local_stt_model_path = stored.local_stt_model_path as
    | string
    | undefined;

  const providerConfig = useAiProvider("stt", current_stt_provider) as
    | AIProviderStorage
    | undefined;

  const localModel = isOnDeviceSttModel(current_stt_provider, current_stt_model)
    ? current_stt_model
    : null;
  const isLocalFile = isLocalFileSttModel(
    current_stt_provider,
    current_stt_model,
  );
  const isLocalModel = !!localModel || isLocalFile;

  const localBatchModel = useQuery({
    ...localSttQueries.isDownloaded("soniqo-parakeet-batch"),
    enabled: isRealtimeLocalModel(current_stt_model),
  });

  const local = useQuery({
    enabled: isLocalModel,
    queryKey: [
      "stt-connection",
      current_stt_provider,
      localModel,
      local_stt_model_path,
    ],
    refetchInterval: (query) =>
      query.state.data?.status === "loading" ? 1000 : false,
    queryFn: async () => {
      if (isLocalFile) {
        const path = local_stt_model_path?.trim();
        if (!path) {
          return {
            status: "not_selected" as const,
            connection: null,
          };
        }

        const started = await localSttCommands.startServerForPath(path);
        if (started.status === "error") {
          return {
            status: "error" as const,
            error: started.error,
            connection: null,
          };
        }

        return {
          status: "ready" as const,
          connection: {
            provider: "local_file" as const,
            model: "local-file" as const,
            baseUrl: started.data,
            apiKey: "",
          },
        };
      }

      if (!localModel) {
        return null;
      }

      const downloaded = await localSttCommands.isModelDownloaded(localModel);
      if (downloaded.status !== "ok" || !downloaded.data) {
        return { status: "not_downloaded" as const, connection: null };
      }

      const serverResult = await localSttCommands.getServerForModel(localModel);

      if (serverResult.status !== "ok") {
        return null;
      }

      const server = serverResult.data;

      if (server?.status === "ready" && server.url) {
        return {
          status: "ready" as const,
          connection: {
            provider: current_stt_provider!,
            model: localModel,
            baseUrl: server.url,
            apiKey: "",
          },
        };
      }

      return {
        status: server?.status ?? "loading",
        connection: null,
      };
    },
  });

  const baseUrl = providerConfig?.base_url?.trim();
  const apiKey = providerConfig?.api_key?.trim();

  const connection = useMemo(() => {
    if (!current_stt_provider || !current_stt_model) {
      return null;
    }

    if (isLocalModel) {
      return local.data?.connection ?? null;
    }

    if (!baseUrl || !apiKey) {
      return null;
    }

    return {
      provider: current_stt_provider,
      model: current_stt_model,
      baseUrl,
      apiKey,
    };
  }, [
    current_stt_provider,
    current_stt_model,
    isLocalModel,
    local.data,
    baseUrl,
    apiKey,
  ]);

  return {
    conn: connection,
    local,
    localBatchDiarizationAvailable: localBatchModel.data === true,
    isLocalModel,
  };
};
