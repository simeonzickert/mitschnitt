import type { ChatTransport, UIMessage } from "ai";

// Fork: upstream wrapped every chat send in a CloudSync "activity lease" so the
// vendor's sync daemon would not run mid-write. That backend is gone, so the
// lease machinery went with it. What stays is the part that is real chat
// behaviour: the preflight that persists the outgoing user message before the
// request leaves, and the abort handling around it.
export type GuardedChatPreflight = {
  run: (
    trackCompletion: (completion: Promise<unknown>) => void,
  ) => void | Promise<void>;
  persistOnCancel: boolean;
};

const noopTrackCompletion = () => {};

function abortError() {
  const error = new Error("Chat request aborted");
  error.name = "AbortError";
  return error;
}

export function withChatTransportPreflight<UI_MESSAGE extends UIMessage>(
  transport: ChatTransport<UI_MESSAGE>,
  {
    beforeSend,
  }: {
    beforeSend?: (logicalKey: string) => GuardedChatPreflight | undefined;
  } = {},
): ChatTransport<UI_MESSAGE> {
  return {
    sendMessages: async (options) => {
      let userMessage: UI_MESSAGE | undefined;
      for (let i = options.messages.length - 1; i >= 0; i--) {
        if (options.messages[i].role === "user") {
          userMessage = options.messages[i];
          break;
        }
      }
      if (!userMessage) {
        throw new Error("Cannot send a chat request without a user message");
      }

      const preflight = beforeSend?.(userMessage.id);
      const isAborted = () => Boolean(options.abortSignal?.aborted);

      if (preflight && (preflight.persistOnCancel || !isAborted())) {
        await preflight.run(noopTrackCompletion);
      }

      if (isAborted()) {
        throw abortError();
      }

      return await transport.sendMessages(options);
    },
    reconnectToStream: (options) => transport.reconnectToStream(options),
  };
}
