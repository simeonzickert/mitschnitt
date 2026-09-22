import { generateId, type LanguageModel, streamText } from "ai";

import { commands as templateCommands } from "@anlg/plugin-template";

import type { TaskArgsMapTransformed, TaskConfig } from ".";
import { appendPreferredNamesGuidance } from "./preferred-names";

const AI_GENERATION_MAX_RETRIES = 4;
// Der Titel selbst braucht ein Dutzend Tokens. Die Decke deckt aber Denken UND
// Antwort ab: ein Modell, das vor der Antwort nachdenkt, verbraucht 128 im
// Denken, die Antwort beginnt nie, und zurueck kommt ein leerer Text ohne
// Fehlermeldung. Gemessen am 03.09.2026 an einem anderen Modell: 21.358
// Denk-Tokens, null Zeichen Antwort, volle Rechnung -- an einer Testfrage
// antwortete dasselbe Modell mit 87 % Denkanteil.
//
// Ein zu hohes Limit kostet nichts, weil nur erzeugte Tokens abgerechnet
// werden. Die Fehlerrichtung ist damit eindeutig, und die Decke wird nach dem
// Denkanteil bemessen, nicht nach der erwarteten Titellaenge.
const TITLE_MAX_OUTPUT_TOKENS = 2_048;

export const titleWorkflow: Pick<
  TaskConfig<"title">,
  "executeWorkflow" | "transforms"
> = {
  executeWorkflow,
  transforms: [],
};

async function* executeWorkflow(params: {
  model: LanguageModel;
  args: TaskArgsMapTransformed["title"];
  onProgress: (step: any) => void;
  signal: AbortSignal;
}) {
  const { model, args, onProgress, signal } = params;

  const system = await getSystemPrompt(args);
  const prompt = await getUserPrompt(args);

  onProgress({ type: "generating" });

  const id = generateId();
  const result = streamText({
    model,
    system,
    prompt,
    abortSignal: signal,
    maxRetries: AI_GENERATION_MAX_RETRIES,
    maxOutputTokens: TITLE_MAX_OUTPUT_TOKENS,
  });

  for await (const chunk of result.textStream) {
    yield {
      type: "text-delta" as const,
      id,
      text: chunk,
    };
  }
}

async function getSystemPrompt(args: TaskArgsMapTransformed["title"]) {
  const result = await templateCommands.render({
    titleSystem: {
      language: args.language,
    },
  });

  if (result.status === "error") {
    throw new Error(result.error);
  }

  return appendPreferredNamesGuidance(result.data, args.dictionaryTerms);
}

async function getUserPrompt(args: TaskArgsMapTransformed["title"]) {
  const { enhancedNote, session, participants } = args;

  const result = await templateCommands.render({
    titleUser: {
      enhancedNote,
      // Derselbe Kontext, den die Zusammenfassung bekommt. Ohne ihn hat das
      // Modell keinen Anhaltspunkt, ob ein Name in der Notiz ueberhaupt
      // stimmt -- und erfindet einen aehnlich klingenden.
      session,
      participants,
    },
  });

  if (result.status === "error") {
    throw new Error(result.error);
  }

  return result.data;
}
