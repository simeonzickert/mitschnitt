// Versionsstempel auf einem Einwilligungs-Nachweis: er sagt, WELCHER
// Hinweistext dem Gegenueber im Meeting-Chat gezeigt wurde. Er wird NUR
// zusammen mit dem Text bewegt -- nie als Umbenennung, nie kosmetisch. Sonst
// behaupten neue Zeilen eine Fassung, die es nie gab, und der Nachweis wird
// gegenueber den Bestandszeilen unvergleichbar.
//
// Am 01.09.2026 bewegt: der Text (MEETING_DISCLOSURE_MESSAGE in
// meeting-disclosure.ts) trug die Adresse `https://anarlog.so` und hat sie
// damit in FREMDE Meeting-Chats geschrieben -- die sichtbarste Marken-Spur
// ueberhaupt, weil sie andere Menschen erreicht. Die Adresse ist raus, also
// muss der Stempel mit. Bestandszeilen behalten `anarlog-disclosure-v1` und
// bleiben damit korrekt dem Text zugeordnet, den sie wirklich gezeigt haben.
//
// Der ALTE Wert steht ausserdem als DEFAULT in der Migration
// 20260821140000 (crates/db-app/migrations) und bleibt dort mit Absicht
// stehen: er beschreibt den Text, der zum Zeitpunkt jener Migration lief.
// Die CHECK-Bedingung dort prueft nur die LAENGE (1 bis 64) und keine
// Werteliste -- ein neuer Wert laeuft also nicht auf, sondern wird
// stillschweigend akzeptiert. Genau deshalb steht die Begruendung hier.
export const MEETING_DISCLOSURE_MESSAGE_VERSION = "mitschnitt-disclosure-v1";

export type DisclosureDelivery = "sent" | "not_sent" | "cancelled";

export type DisclosurePlatform =
  | "slack_huddle"
  | "zoom"
  | "google_meet"
  | "teams"
  | "webex"
  | "browser"
  | "unknown";

export type DisclosureAttempt = {
  id: string;
  sessionId: string;
  attemptedAt: string;
  platform: DisclosurePlatform;
  surface: string;
  messageVersion: string;
  message: string;
  delivery: DisclosureDelivery;
  failureReason: string;
};

export type ParticipantConsentStatus = "unknown" | "consented" | "declined";

export type ParticipantConsentSource =
  | "explicit_chat_reply"
  | "explicit_ui"
  | "unseen";

export type ParticipantConsent = {
  sessionId: string;
  participantKey: string;
  status: ParticipantConsentStatus;
  source: ParticipantConsentSource;
  updatedAt: string;
};

export type SessionListeningPolicy = "continue" | "stop_declined";

const DECLINE_PATTERN =
  /\b((i\s+)?(do\s+not|don't|does\s+not|doesn't)\s+consent|stop\s+recording)\b/i;
const CONSENT_PATTERN = /\b(i\s+consent|i\s+agree\s+to\s+(being\s+)?record)/i;

export function applyDisclosureAttempt(
  consents: readonly ParticipantConsent[],
  _attempt: DisclosureAttempt,
): ParticipantConsent[] {
  return [...consents];
}

export function applyLateJoiner(
  consents: readonly ParticipantConsent[],
  sessionId: string,
  participantKey: string,
  updatedAt: string,
): ParticipantConsent[] {
  if (
    consents.some(
      (consent) =>
        consent.sessionId === sessionId &&
        consent.participantKey === participantKey,
    )
  ) {
    return [...consents];
  }
  return [
    ...consents,
    {
      sessionId,
      participantKey,
      status: "unknown",
      source: "unseen",
      updatedAt,
    },
  ];
}

export function applyExplicitConsentResponse(
  consents: readonly ParticipantConsent[],
  next: ParticipantConsent,
): ParticipantConsent[] {
  if (next.source === "unseen") {
    throw new Error("explicit consent cannot use the unseen source");
  }
  const without = consents.filter(
    (consent) =>
      !(
        consent.sessionId === next.sessionId &&
        consent.participantKey === next.participantKey
      ),
  );
  return [...without, next];
}

export function interpretChatAsConsentResponse(
  text: string,
  disclosureMessage: string,
): ParticipantConsentStatus | null {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return null;
  }
  if (normalized === disclosureMessage.replace(/\s+/g, " ").trim()) {
    return null;
  }
  if (DECLINE_PATTERN.test(normalized)) {
    return "declined";
  }
  if (CONSENT_PATTERN.test(normalized)) {
    return "consented";
  }
  return null;
}

export function sessionListeningPolicy(
  consents: readonly ParticipantConsent[],
): SessionListeningPolicy {
  return consents.some((consent) => consent.status === "declined")
    ? "stop_declined"
    : "continue";
}

export function sessionHasLegalConsent(
  consents: readonly ParticipantConsent[],
  disclosureAttempts: readonly DisclosureAttempt[],
): boolean {
  void consents;
  void disclosureAttempts;
  return false;
}
