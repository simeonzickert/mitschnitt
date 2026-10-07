import { Trans, useLingui } from "@lingui/react/macro";
import { useState } from "react";

import { Accordion } from "@anlg/ui/components/ui/accordion";
import { Separator } from "@anlg/ui/components/ui/separator";

import { useSttSettings } from "./context";
import {
  STT_LOCAL_PROVIDER_IDS,
  STT_TOP_PROVIDER_IDS,
  useConfiguredMapping,
} from "./select";
import { ProviderId, PROVIDERS } from "./shared";

import {
  filterProviders,
  NonAnarlogProviderCard,
  ProviderGroupLabel,
  ProviderSearch,
  StyledStreamdown,
} from "~/settings/ai/shared";
import {
  MoreGroupHeader,
  useCollapsedMore,
} from "~/settings/ai/shared/more-group";
import { splitLocalTopMore } from "~/settings/ai/shared/provider-groups";
import { getConfiguredProviderIds } from "~/settings/ai/shared/selection";
import { useConfigValue } from "~/shared/config";

// Der Import aus "./select" ist gewollt: dieselbe Quelle (splitLocalTopMore,
// die Local/Top-Konstanten, der "configured"-Status) gruppiert sowohl das
// Dropdown dort als auch diese Karten-Liste hier -- nicht zweimal sortieren
// (28.09.2026, Befund: die Karten waren "bunt gemischt").
export function ConfigureProviders() {
  const { accordionValue, setAccordionValue, pendingProvider } =
    useSttSettings();
  const currentProvider = useConfigValue("current_stt_provider");
  const [search, setSearch] = useState("");
  const { t } = useLingui();
  const { providers: configuredProviders } = useConfiguredMapping();
  const providers = filterProviders(
    PROVIDERS.filter((provider) => !("builtIn" in provider)),
    search,
  );
  const configuredProviderIds = getConfiguredProviderIds(
    providers,
    configuredProviders,
    currentProvider,
  );
  const { local, top, more } = splitLocalTopMore(
    providers,
    STT_LOCAL_PROVIDER_IDS,
    STT_TOP_PROVIDER_IDS,
    { selectedId: currentProvider, configuredIds: configuredProviderIds },
  );
  // "More" is collapsed by default; selected/configured providers stay
  // visible and an active search shows every hit.
  const moreGroup = useCollapsedMore(
    more,
    {
      selectedId: currentProvider,
      // A provider picked in the dropdown but not saved yet stays visible.
      configuredIds: pendingProvider
        ? [...configuredProviderIds, pendingProvider]
        : configuredProviderIds,
    },
    search.trim().length > 0,
  );

  const renderCard = (provider: (typeof providers)[number]) => (
    <NonAnarlogProviderCard
      key={provider.id}
      config={provider}
      providerType="stt"
      providers={PROVIDERS}
      providerContext={<ProviderContext providerId={provider.id} />}
      currentProvider={currentProvider}
    />
  );

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <h3 className="text-md font-sans font-semibold">
          <Trans>Configure Providers</Trans>
        </h3>
        <ProviderSearch value={search} onChange={setSearch} />
      </div>
      <Accordion
        type="single"
        collapsible
        className="flex flex-col gap-3"
        value={accordionValue}
        onValueChange={setAccordionValue}
      >
        {local.length > 0 ? (
          <>
            <ProviderGroupLabel>{t`Local`}</ProviderGroupLabel>
            {local.map(renderCard)}
            <Separator />
          </>
        ) : null}
        {top.length > 0 ? (
          <>
            <ProviderGroupLabel>{t`Cloud`}</ProviderGroupLabel>
            {top.map(renderCard)}
          </>
        ) : null}
        {more.length > 0 ? (
          <>
            {top.length > 0 || local.length > 0 ? <Separator /> : null}
            <MoreGroupHeader
              hiddenCount={moreGroup.hiddenCount}
              expanded={moreGroup.expanded}
              canToggle={moreGroup.canToggle}
              onToggle={moreGroup.toggle}
            />
            {moreGroup.visible.map(renderCard)}
          </>
        ) : null}
      </Accordion>
      {providers.length === 0 ? (
        <p className="text-muted-foreground py-8 text-center text-sm">
          <Trans>No providers found.</Trans>
        </p>
      ) : null}
    </div>
  );
}

function ProviderContext({ providerId }: { providerId: ProviderId }) {
  const content =
    providerId === "anarlog"
      ? "**Mitschnitt Cloud** routes request to the **best available model** for highest accuracy and performance."
      : providerId === "deepgram"
        ? `Use [Deepgram](https://deepgram.com) for transcriptions. \
    If you want to use a [Dedicated](https://developers.deepgram.com/reference/custom-endpoints#deepgram-dedicated-endpoints)
    or [EU](https://developers.deepgram.com/reference/custom-endpoints#eu-endpoints) endpoint,
    you can do that in the **advanced** section.`
        : providerId === "soniox"
          ? `Use [Soniox](https://soniox.com) for transcriptions.`
          : providerId === "assemblyai"
            ? `Use [AssemblyAI](https://www.assemblyai.com) for transcriptions.`
            : providerId === "gladia"
              ? `Use [Gladia](https://www.gladia.io) for transcriptions.`
              : providerId === "openai"
                ? `Use [OpenAI](https://openai.com) for transcriptions.`
                : providerId === "openrouter"
                  ? `Use [OpenRouter](https://openrouter.ai) to transcribe with supported speech-to-text models through one API key. OpenRouter transcription runs after recording.`
                  : providerId === "dashscope"
                    ? `Use Alibaba Cloud Model Studio's Qwen ASR for **live transcription**. The default endpoint is the Singapore region; change it under Advanced when your API key belongs to another region.`
                    : providerId === "zai"
                      ? `Use [Z.AI GLM ASR](https://docs.z.ai/guides/audio/glm-asr-2512) for batch transcription. Mitschnitt automatically splits recordings to fit Z.AI's 30-second upload limit.`
                      : providerId === "siliconflow"
                        ? `Use [SiliconFlow](https://docs.siliconflow.com/en/api-reference/audio/create-audio-transcriptions) for batch transcription. The default endpoint is the international service; use \`https://api.siliconflow.cn/v1\` under Advanced for a China-region API key.`
                        : providerId === "cloudflare_workers_ai"
                          ? `Use a [Cloudflare Workers AI](https://developers.cloudflare.com/workers-ai/) endpoint that exposes Deepgram-compatible Nova-3 transcription.`
                          : providerId === "fireworks"
                            ? `Use [Fireworks AI](https://fireworks.ai) for transcriptions.`
                            : providerId === "mistral"
                              ? `Use [Mistral](https://mistral.ai) for transcriptions. Keep the Base URL as \`https://api.mistral.ai/v1\` (Reset under Advanced if you pasted a transcriptions endpoint). **Voxtral Mini Transcribe 2** transcribes after recording; the realtime model is for live captions.`
                              : providerId === "cohere"
                                ? `Use [Cohere Transcribe](https://docs.cohere.com/docs/transcribe) for batch transcription. Files must be 25 MB or smaller and use one selected language. Cohere does not return timestamps or speaker labels, so Mitschnitt estimates word timing.`
                                : providerId === "google_generative_ai"
                                  ? `Use [Gemini 3.5 Transcribe](https://blog.google/innovation-and-ai/models-and-research/gemini-models/gemini-3-5-transcribe/) with a Google AI Studio API key. **3.5 Transcribe Live** captions during recording (preview sessions last up to 10 minutes). **3.5 Transcribe** runs after recording with speaker labels and word timestamps; Mitschnitt splits files past 15 minutes.`
                                  : providerId === "google_cloud"
                                    ? `Use [Google Cloud Speech-to-Text](https://cloud.google.com/speech-to-text) synchronous recognition for recordings up to one minute and 10 MB. Paste an OAuth access token in the API key field; refresh it when it expires.`
                                    : providerId === "azure_speech"
                                      ? `Use [Azure AI Speech](https://learn.microsoft.com/azure/ai-services/speech-service/rest-speech-to-text) fast transcription. Enter the regional Speech resource endpoint as the Base URL and its subscription key as the API key.`
                                      : providerId === "aws_transcribe"
                                        ? `Amazon Transcribe's native file API requires SigV4 plus an S3 object. Enter an OpenAI-compatible gateway URL that performs that AWS authentication and upload, then paste the gateway token as the API key.`
                                        : providerId === "speechmatics"
                                          ? `Use [Speechmatics](https://docs.speechmatics.com/speech-to-text/batch/quickstart) enhanced batch transcription. The default endpoint uses the EU region and can be changed under Advanced.`
                                          : providerId === "revai"
                                            ? `Use [Rev AI](https://docs.rev.ai/api/asynchronous/get-started) asynchronous transcription. Mitschnitt uploads the recording, waits for the job, and retrieves word timestamps and speaker labels.`
                                            : providerId === "custom"
                                              ? `We only support **Deepgram compatible** endpoints for now.`
                                              : "";

  if (!content.trim()) {
    return null;
  }

  return <StyledStreamdown className="mb-2">{content.trim()}</StyledStreamdown>;
}
