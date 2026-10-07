import {
  getDefaultSttModel,
  getPreferredProviderModel,
} from "~/stt/model-selection";

export { getDefaultSttModel, getPreferredProviderModel };

export async function getLanguageSupportIssue(
  languages: readonly string[],
  isSupported: (languages: readonly string[]) => Promise<boolean>,
) {
  if (await isSupported(languages)) {
    return null;
  }

  const supportByLanguage = await Promise.all(
    languages.map(async (language) => ({
      language,
      supported: await isSupported([language]),
    })),
  );

  return {
    unsupportedLanguages: supportByLanguage
      .filter(({ supported }) => !supported)
      .map(({ language }) => language),
  };
}

export function getDefaultSttSelection(
  providerIds: readonly string[],
  statuses: Record<
    string,
    {
      configured: boolean;
      models: { id: string; isDownloaded?: boolean }[];
    }
  >,
  currentProvider?: string,
  currentModel?: string,
) {
  for (const provider of providerIds) {
    const status = statuses[provider];
    if (!status?.configured) {
      continue;
    }

    const model = getPreferredProviderModel(
      provider === currentProvider ? currentModel : undefined,
      status.models,
      { allowSavedModelWithoutChoices: provider === "custom" },
    );

    if (model) {
      return { provider, model };
    }
  }

  return null;
}

export function resolveLiveLanguageSupportMode({
  isOnDeviceModel,
  useLiveOnDeviceModel,
  liveSupported,
}: {
  isOnDeviceModel: boolean;
  useLiveOnDeviceModel: boolean;
  liveSupported: boolean | undefined;
}): boolean | undefined {
  return isOnDeviceModel
    ? useLiveOnDeviceModel && liveSupported
    : liveSupported;
}

// A provider picked in the dropdown without a default model is only
// "pending". Once the saved selection is that same provider and has a model
// (e.g. saved through its card), the pending state is stale and must go, or
// the dropdown keeps showing the provider with an empty model.
export function resolvePendingProvider<T extends string>(
  pending: T | null,
  savedProvider: string | undefined,
  savedModel: string | undefined,
): T | null {
  if (pending && savedProvider === pending && savedModel) {
    return null;
  }
  return pending;
}
