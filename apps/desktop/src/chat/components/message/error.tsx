import { useLingui } from "@lingui/react/macro";
import { ArrowCounterClockwise } from "@phosphor-icons/react";

import { ActionButton, MessageBubble, MessageContainer } from "./shared";

// Bis zum 01.09.2026 haengte die Fehlerblase bei Kontextlaengen-Fehlern einen
// "Learn how to fix this"-Knopf an, der `${VITE_APP_URL}/docs/faq/...` oeffnete:
// die Doku-Seite des Originals (im Fork-Bau: localhost:3000, also nichts).
// Die Erkennung bleibt, damit der Hinweis im Text stehen kann, sobald es eine
// eigene Anlaufstelle gibt.
function isContextLengthError(message: string): boolean {
  const lowerMessage = message.toLowerCase();
  return (
    (lowerMessage.includes("n_keep") && lowerMessage.includes("n_ctx")) ||
    (lowerMessage.includes("context") && lowerMessage.includes("exceeds")) ||
    lowerMessage.includes("context length") ||
    lowerMessage.includes("context size")
  );
}

export function ErrorMessage({
  error,
  onRetry,
}: {
  error: Error;
  onRetry?: () => void;
}) {
  const { t } = useLingui();
  const showContextLengthHelp = isContextLengthError(error.message);

  return (
    <MessageContainer align="start">
      <MessageBubble variant="error" withActionButton={!!onRetry}>
        <p className="text-sm">{error.message}</p>
        {showContextLengthHelp && (
          <p className="mt-2 text-xs text-red-700">
            {t`The conversation no longer fits the model's context window. Start a new chat or pick a model with a larger context.`}
          </p>
        )}
        {onRetry && (
          <ActionButton
            onClick={onRetry}
            variant="error"
            icon={ArrowCounterClockwise}
            label={t`Retry`}
          />
        )}
      </MessageBubble>
    </MessageContainer>
  );
}
