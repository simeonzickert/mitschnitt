import {
  type ImagePart,
  type LanguageModel,
  smoothStream,
  streamText,
  type TextPart,
} from "ai";

import { commands as templateCommands } from "@anlg/plugin-template";

import type { TaskArgsMapTransformed, TaskConfig } from ".";
import type { EnhanceImageContext } from "./enhance-images";
import { createEnhanceValidator } from "./enhance-validator";
import { appendPreferredNamesGuidance } from "./preferred-names";

import {
  formatSummaryLengthModeGuidance,
  formatSummaryLengthGuidance,
  getSummaryLengthPolicy,
} from "~/services/enhancer/summary-length";
import { normalizeBulletPoints } from "~/store/zustand/ai-task/shared/transform_impl";
import { withEarlyValidationRetry } from "~/store/zustand/ai-task/shared/validate";
import { assertCanonicalTemplateSections } from "~/templates/codec";

const AI_GENERATION_MAX_RETRIES = 4;
const ANTHROPIC_SUMMARY_MAX_OUTPUT_TOKENS = 64_000;
const DEFAULT_SUMMARY_MAX_OUTPUT_TOKENS = 32_000;
const APPLE_FOUNDATION_PROVIDER = "apple_foundation";
const IMAGE_CONTEXT_NOTE =
  "Attached note images are included as visual context. Use visible text, diagrams, screenshots, and other image content when it materially improves the summary.";

export const enhanceWorkflow: Pick<
  TaskConfig<"enhance">,
  "executeWorkflow" | "transforms"
> = {
  executeWorkflow,
  transforms: [
    normalizeBulletPoints(),
    smoothStream({ delayInMs: 250, chunking: "line" }),
  ],
};

// Anthropic requires max_tokens and the SDK clamps it to known model limits.
// For every other remote or local-server provider an explicit cap is set:
// OpenRouter checks the account balance against the model maximum when
// max_tokens is missing, local OpenAI-compatible servers sometimes default to
// very small limits, and a cap also bounds repetition loops of local models.
// Apple Foundation is the on-device model with a 4096-token context, so it
// gets no explicit value.
export function getSummaryMaxOutputTokens(
  provider: string,
): number | undefined {
  if (provider.startsWith("anthropic")) {
    return ANTHROPIC_SUMMARY_MAX_OUTPUT_TOKENS;
  }
  if (provider === APPLE_FOUNDATION_PROVIDER) {
    return undefined;
  }
  return DEFAULT_SUMMARY_MAX_OUTPUT_TOKENS;
}

async function* executeWorkflow(params: {
  model: LanguageModel;
  args: TaskArgsMapTransformed["enhance"];
  onProgress: (step: any) => void;
  signal: AbortSignal;
}) {
  const { model, args, onProgress, signal } = params;

  const system = await getSystemPrompt(args);
  const prompt = withLengthGuidance(
    withImageContextNote(await getUserPrompt(args), args.imageContext.length),
    args.transcripts,
    Boolean(args.template?.sections.length),
    args.summaryLength,
  );

  yield* generateSummary({
    model,
    args,
    system,
    prompt,
    onProgress,
    signal,
  });
}

async function getSystemPrompt(args: TaskArgsMapTransformed["enhance"]) {
  const result = await templateCommands.render({
    enhanceSystem: {
      language: args.language,
      formatOverride: args.formatOverride,
    },
  });

  if (result.status === "error") {
    throw new Error(result.error);
  }

  const modeGuidance = formatSummaryLengthModeGuidance(
    args.summaryLength,
    Boolean(args.template?.sections.length),
  );

  return appendPreferredNamesGuidance(
    `${result.data}

# Summary Mode

${modeGuidance}`,
    args.dictionaryTerms,
  );
}

async function getUserPrompt(args: TaskArgsMapTransformed["enhance"]) {
  const {
    session,
    participants,
    template: rawTemplate,
    transcripts,
    preMeetingMemo,
    postMeetingMemo,
  } = args;
  const template = rawTemplate
    ? {
        ...rawTemplate,
        sections: assertCanonicalTemplateSections(
          rawTemplate.sections,
          "enhance render template.sections",
        ),
      }
    : null;

  const result = await templateCommands.render({
    enhanceUser: {
      session,
      participants,
      template,
      transcripts,
      preMeetingMemo,
      postMeetingMemo,
    },
  });

  if (result.status === "error") {
    throw new Error(result.error);
  }

  return result.data;
}

async function* generateSummary(params: {
  model: LanguageModel;
  args: TaskArgsMapTransformed["enhance"];
  system: string;
  prompt: string;
  onProgress: (step: any) => void;
  signal: AbortSignal;
}) {
  const { model, args, system, prompt, onProgress, signal } = params;

  onProgress({ type: "generating" });

  const validator = createEnhanceValidator(args.template, {
    overrideTemplateFormatting: Boolean(args.formatOverride.trim()),
  });

  yield* withEarlyValidationRetry(
    (retrySignal, { previousFeedback }) => {
      let enhancedPrompt = prompt;

      if (previousFeedback) {
        enhancedPrompt = `${prompt}

IMPORTANT: Previous attempt failed. ${previousFeedback}`;
      }

      const combinedController = new AbortController();

      const abortFromOuter = () => combinedController.abort();
      const abortFromRetry = () => combinedController.abort();

      signal.addEventListener("abort", abortFromOuter);
      retrySignal.addEventListener("abort", abortFromRetry);

      const result = streamText({
        model,
        system,
        ...createPromptInput(enhancedPrompt, args.imageContext),
        abortSignal: combinedController.signal,
        maxRetries: AI_GENERATION_MAX_RETRIES,
        maxOutputTokens: getSummaryMaxOutputTokens(
          typeof model === "string" ? "" : model.provider,
        ),
      });
      return withCleanup(result.fullStream, () => {
        signal.removeEventListener("abort", abortFromOuter);
        retrySignal.removeEventListener("abort", abortFromRetry);
      });
    },
    validator,
    {
      minChar: 10,
      maxChar: 30,
      maxRetries: 2,
      onRetry: (attempt, feedback) => {
        onProgress({ type: "retrying", attempt, reason: feedback });
      },
      onRetrySuccess: () => {
        onProgress({ type: "generating" });
      },
      onGiveUp: () => {
        onProgress({ type: "generating" });
      },
    },
  );
}

async function* withCleanup<T>(
  stream: AsyncIterable<T>,
  cleanup: () => void,
): AsyncIterable<T> {
  try {
    yield* stream;
  } finally {
    cleanup();
  }
}

function withImageContextNote(prompt: string, imageCount: number): string {
  if (imageCount === 0) {
    return prompt;
  }

  return `${prompt}

${IMAGE_CONTEXT_NOTE}`;
}

function withLengthGuidance(
  prompt: string,
  transcripts: TaskArgsMapTransformed["enhance"]["transcripts"],
  hasTemplateSections: boolean,
  summaryLength: TaskArgsMapTransformed["enhance"]["summaryLength"],
): string {
  if (hasTemplateSections) {
    return prompt;
  }

  const guidance = formatSummaryLengthGuidance(
    getSummaryLengthPolicy(transcripts, summaryLength),
  );
  if (!guidance) return prompt;

  return `${prompt}

${guidance}`;
}

function createPromptInput(
  prompt: string,
  imageContext: EnhanceImageContext[],
):
  | { prompt: string }
  | {
      messages: Array<{ role: "user"; content: Array<TextPart | ImagePart> }>;
    } {
  if (imageContext.length === 0) {
    return { prompt };
  }

  return {
    messages: [
      {
        role: "user",
        content: [
          { type: "text", text: prompt },
          ...imageContext.map(
            (image): ImagePart => ({
              type: "image",
              image: image.base64,
              mediaType: image.mimeType,
            }),
          ),
        ],
      },
    ],
  };
}
