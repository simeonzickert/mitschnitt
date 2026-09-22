import { createAnthropic } from "@ai-sdk/anthropic";
import { createAzure } from "@ai-sdk/azure";
import { createGoogleGenerativeAI } from "@ai-sdk/google";
import { createOpenAI } from "@ai-sdk/openai";
import { createOpenAICompatible } from "@ai-sdk/openai-compatible";
import { createOpenRouter } from "@openrouter/ai-sdk-provider";
import { fetch as tauriFetch } from "@tauri-apps/plugin-http";
import { extractReasoningMiddleware, wrapLanguageModel } from "ai";
import { useMemo } from "react";

import type { AIProviderStorage } from "@anlg/store";

import { createAppleFoundationModel } from "../apple-foundation-model";
import { streamOnlyGenerationMiddleware } from "../stream-only-generation";

import {
  type Provider,
  type ProviderId,
  PROVIDERS,
} from "~/settings/ai/llm/shared";
import {
  CHATGPT_API_BASE_URL,
  createSubscriptionFetch,
  usesSubscriptionFetch,
} from "~/settings/ai/llm/subscriptions";
import {
  getProviderSelectionBlockers,
  type ProviderEligibilityContext,
} from "~/settings/ai/shared/eligibility";
import { useAiProvider } from "~/settings/providers";
import { useConfigValues } from "~/shared/config";

type LanguageModelV3 = Parameters<typeof wrapLanguageModel>[0]["model"];

export type CharTask = "chat" | "enhance" | "title";

type LLMConnectionInfo = {
  providerId: ProviderId;
  modelId: string;
  baseUrl: string;
  apiKey: string;
};

export type LLMConnectionStatus =
  | { status: "pending"; reason: "missing_provider" }
  | { status: "pending"; reason: "missing_model"; providerId: ProviderId }
  | { status: "error"; reason: "provider_not_found"; providerId: string }
  | {
      status: "error";
      reason: "missing_config";
      providerId: ProviderId;
      missing: Array<"base_url" | "api_key">;
    }
  | { status: "success"; providerId: ProviderId; isHosted: boolean };

type LLMConnectionResult = {
  conn: LLMConnectionInfo | null;
  status: LLMConnectionStatus;
};

export const normalizeLLMProviderId = (providerId: string): string =>
  providerId === "hyprnote" ? "anarlog" : providerId;

export const useLanguageModel = (_task?: CharTask): LanguageModelV3 | null => {
  const { conn } = useLLMConnection();

  return useMemo(() => {
    if (!conn) return null;

    return createLanguageModel(conn);
  }, [conn]);
};

export const useLLMConnection = (): LLMConnectionResult => {
  const { current_llm_provider, current_llm_model } = useConfigValues([
    "current_llm_provider",
    "current_llm_model",
  ] as const);
  const providerConfig = useAiProvider("llm", current_llm_provider) as
    | AIProviderStorage
    | undefined;

  return useMemo<LLMConnectionResult>(
    () =>
      resolveLLMConnection({
        providerId: current_llm_provider,
        modelId: current_llm_model,
        providerConfig,
      }),
    [current_llm_model, current_llm_provider, providerConfig],
  );
};

export const useLLMConnectionStatus = (): LLMConnectionStatus => {
  const { status } = useLLMConnection();
  return status;
};

/**
 * Providers whose client takes no base URL at all. Everything else ends up in
 * an HTTP client, and an HTTP client with `baseURL: ""` resolves its paths
 * against the webview origin -- a request that looks like it went somewhere
 * and never reached a model. The definitions guard against that today
 * (`requires_config: ["base_url"]` where the URL is not authored), this is
 * the second net at the one place a connection is handed out (F7).
 */
const PROVIDERS_WITHOUT_BASE_URL: ReadonlySet<string> = new Set([
  "apple_foundation",
]);

export const resolveLLMConnection = (
  params: {
    providerId: string | undefined;
    modelId: string | undefined;
    providerConfig: AIProviderStorage | undefined;
  },
  providers: readonly Provider[] = PROVIDERS,
): LLMConnectionResult => {
  const { providerId: rawProviderId, modelId, providerConfig } = params;

  if (!rawProviderId) {
    return {
      conn: null,
      status: { status: "pending", reason: "missing_provider" },
    };
  }

  const providerId = normalizeLLMProviderId(rawProviderId) as ProviderId;

  if (!modelId) {
    return {
      conn: null,
      status: { status: "pending", reason: "missing_model", providerId },
    };
  }

  const providerDefinition = providers.find((p) => p.id === providerId);

  if (!providerDefinition) {
    return {
      conn: null,
      status: {
        status: "error",
        reason: "provider_not_found",
        providerId: rawProviderId,
      },
    };
  }

  const baseUrl =
    providerConfig?.base_url?.trim() ||
    providerDefinition.baseUrl?.trim() ||
    "";
  const apiKey = providerConfig?.api_key?.trim() || "";

  const context: ProviderEligibilityContext = {
    config: { base_url: baseUrl, api_key: apiKey },
  };

  const blockers = getProviderSelectionBlockers(
    providerDefinition.requirements,
    context,
  );

  if (blockers.length > 0) {
    const blocker = blockers[0];
    if (blocker.code === "missing_config") {
      return {
        conn: null,
        status: {
          status: "error",
          reason: "missing_config",
          providerId,
          missing: blocker.fields,
        },
      };
    }
  }

  if (!baseUrl && !PROVIDERS_WITHOUT_BASE_URL.has(providerId)) {
    return {
      conn: null,
      status: {
        status: "error",
        reason: "missing_config",
        providerId,
        missing: ["base_url"],
      },
    };
  }

  return {
    conn: { providerId, modelId, baseUrl, apiKey },
    status: { status: "success", providerId, isHosted: false },
  };
};

const wrapWithThinkingMiddleware = (
  model: LanguageModelV3,
): LanguageModelV3 => {
  return wrapLanguageModel({
    model,
    middleware: [
      extractReasoningMiddleware({ tagName: "think" }),
      extractReasoningMiddleware({ tagName: "thinking" }),
    ],
  });
};

const createLanguageModel = (conn: LLMConnectionInfo): LanguageModelV3 => {
  switch (conn.providerId) {
    case "anthropic": {
      const provider = createAnthropic({
        fetch: tauriFetch,
        apiKey: conn.apiKey,
        headers: {
          "anthropic-version": "2023-06-01",
          "anthropic-dangerous-direct-browser-access": "true",
        },
      });
      return wrapWithThinkingMiddleware(provider(conn.modelId));
    }

    case "claude": {
      const oauth = usesSubscriptionFetch(conn.providerId, conn.apiKey);
      const provider = createAnthropic({
        fetch: oauth
          ? createSubscriptionFetch(conn.providerId, conn.apiKey)
          : tauriFetch,
        apiKey: oauth ? "oauth" : conn.apiKey,
        headers: {
          "anthropic-version": "2023-06-01",
          "anthropic-dangerous-direct-browser-access": "true",
        },
      });
      return wrapWithThinkingMiddleware(provider(conn.modelId));
    }

    case "chatgpt": {
      const oauth = usesSubscriptionFetch(conn.providerId, conn.apiKey);
      const provider = createOpenAI({
        fetch: oauth
          ? createSubscriptionFetch(conn.providerId, conn.apiKey)
          : tauriFetch,
        baseURL: oauth ? CHATGPT_API_BASE_URL : conn.baseUrl,
        apiKey: oauth ? "oauth" : conn.apiKey,
      });
      const model = provider.responses(conn.modelId);
      return wrapWithThinkingMiddleware(
        oauth
          ? wrapLanguageModel({
              model,
              middleware: streamOnlyGenerationMiddleware,
            })
          : model,
      );
    }

    case "grok":
    case "github_copilot": {
      const provider = createOpenAICompatible({
        fetch: createSubscriptionFetch(conn.providerId, conn.apiKey),
        name: conn.providerId,
        baseURL: conn.baseUrl,
        apiKey: "oauth",
      });
      return wrapWithThinkingMiddleware(provider.chatModel(conn.modelId));
    }

    case "google_generative_ai": {
      const provider = createGoogleGenerativeAI({
        fetch: tauriFetch,
        baseURL: conn.baseUrl,
        apiKey: conn.apiKey,
      });
      return wrapWithThinkingMiddleware(provider(conn.modelId));
    }

    case "openrouter": {
      const provider = createOpenRouter({
        fetch: tauriFetch,
        apiKey: conn.apiKey,
      });
      return wrapWithThinkingMiddleware(provider.chat(conn.modelId));
    }

    case "openai": {
      const provider = createOpenAI({
        fetch: tauriFetch,
        baseURL: conn.baseUrl,
        apiKey: conn.apiKey,
      });
      return wrapWithThinkingMiddleware(provider(conn.modelId));
    }

    case "azure_openai": {
      const provider = createAzure({
        fetch: tauriFetch,
        baseURL: conn.baseUrl,
        apiKey: conn.apiKey,
      });
      return wrapWithThinkingMiddleware(provider(conn.modelId));
    }

    case "azure_ai": {
      const provider = createOpenAICompatible({
        fetch: tauriFetch,
        name: "azure_ai",
        baseURL: conn.baseUrl,
        apiKey: conn.apiKey,
        headers: { "api-key": conn.apiKey },
      });
      return wrapWithThinkingMiddleware(provider.chatModel(conn.modelId));
    }

    case "ollama": {
      const ollamaOrigin = new URL(conn.baseUrl.replace(/\/v1\/?$/, "")).origin;
      const ollamaFetch: typeof fetch = async (input, init) => {
        const headers = new Headers(init?.headers);
        headers.set("Origin", ollamaOrigin);
        return tauriFetch(input as RequestInfo | URL, {
          ...init,
          headers,
        });
      };
      const provider = createOpenAICompatible({
        fetch: ollamaFetch,
        name: conn.providerId,
        baseURL: conn.baseUrl,
      });
      return wrapWithThinkingMiddleware(provider.chatModel(conn.modelId));
    }

    case "apple_foundation":
      return createAppleFoundationModel(conn.modelId);

    default: {
      const config: Parameters<typeof createOpenAICompatible>[0] = {
        fetch: tauriFetch,
        name: conn.providerId,
        baseURL: conn.baseUrl,
      };
      if (conn.apiKey) {
        config.apiKey = conn.apiKey;
      }
      const provider = createOpenAICompatible(config);
      return wrapWithThinkingMiddleware(provider.chatModel(conn.modelId));
    }
  }
};
