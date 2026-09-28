import type { TranscriptRow } from "./render-transcript";

/**
 * Etikett fuer Bloecke eines Transkripts ohne Sprechertrennung (ZICK-312).
 *
 * Hintergrund: laeuft keine Trennung (z. B. Cloud-Transkription, alles auf
 * einem Kanal), gilt fuer die Anzeige jeder Mikrofon-Block als der Nutzer
 * selbst. Im Prompt stand dann vor JEDER Zeile sein Name -- eine sichere
 * Zuschreibung, die es nicht gibt, und das Modell verteilte daraus Aussagen und
 * Aufgaben auf die Teilnehmerliste. Gemessen am Gespraech vom 25.09.2026
 * (OpenAI, 5.516 Woerter, alle Kanal 0, keine Sprecherhinweise): die
 * Zusammenfassung schrieb Entscheidungen namentlich zwei Personen zu.
 *
 * Der Wortlaut ist mit den Regeln in `enhance.system.md.jinja` und
 * `chat.system.md.jinja` gekoppelt, die genau dieses Etikett nennen. Die
 * Rust-Fassung derselben Regel (Markdown-Spiegel) steht in
 * `crates/transcript/src/separation.rs`; aendert sich eine, muss die andere
 * folgen.
 */
export const UNSEPARATED_SPEAKER_LABEL = "Unknown speaker";

/**
 * Traegt das Transkript eine echte Sprechertrennung?
 *
 * Getrennt heisst: Woerter auf mehr als einem Kanal (Mikrofon und Systemton),
 * mehr als ein Anbieter-Sprecher oder mehr als ein automatisch zugeordneter
 * Mensch. Ohne Woerter laesst sich nichts beurteilen; dann bleibt alles, wie
 * es ist. Woerter OHNE Kanalangabe gelten dagegen als ungetrennt (Forge M5,
 * fail-closed): der Renderer legt sie auf Kanal 0, und der hiesse "der
 * Nutzer".
 *
 * Eine Handzuordnung zaehlt hier NICHT (Opus-Zweitblick 26.09.2026): sie
 * benennt die Bloecke, die ein Mensch zugeordnet hat, und schaltete frueher
 * mit einem einzigen Klick alle Namen im Transkript wieder ein. Welche
 * Bloecke sie traegt, sagt `manuallyAssignedWordIds`.
 */
export function transcriptHasSpeakerSeparation(rows: TranscriptRow[]): boolean {
  const channels = new Set<number>();
  const providerSpeakers = new Set<string>();
  const assignedHumans = new Set<string>();
  let hasWords = false;

  for (const row of rows) {
    for (const word of row.words ?? []) {
      hasWords = true;
      if (typeof word.channel === "number") {
        channels.add(word.channel);
      }
    }

    for (const hint of row.speaker_hints ?? []) {
      const value = parseHintValue(hint.value);
      if (
        hint.type === "provider_speaker_index" &&
        typeof value?.speaker_index === "number"
      ) {
        providerSpeakers.add(`${String(value.channel)}:${value.speaker_index}`);
      }
      if (
        hint.type === "automatic_speaker_assignment" &&
        typeof value?.human_id === "string"
      ) {
        assignedHumans.add(value.human_id);
      }
    }
  }

  // Ohne Woerter laesst sich nichts beurteilen. Ohne Kanalangabe schon: dann
  // traegt nur ein Anbieter- oder Zuordnungsbefund eine Trennung.
  if (!hasWords) {
    return true;
  }

  return (
    channels.size > 1 || providerSpeakers.size > 1 || assignedHumans.size > 1
  );
}

function parseHintValue(value: unknown): Record<string, unknown> | null {
  let parsed = value;
  if (typeof value === "string") {
    try {
      parsed = JSON.parse(value);
    } catch {
      return null;
    }
  }
  return parsed && typeof parsed === "object" && !Array.isArray(parsed)
    ? (parsed as Record<string, unknown>)
    : null;
}

/**
 * Sollen die Sprecher-Etiketten vor dem Modell neutralisiert werden?
 *
 * Ja, wenn andere Menschen dabei waren (nicht nur der Nutzer selbst) und das
 * Transkript keine Trennung traegt. Gilt fuer JEDEN Weg, der Transkript-Zeilen
 * an ein Sprachmodell gibt: Zusammenfassung und Chat.
 */
export function shouldNeutralizeSpeakerLabels(
  rows: TranscriptRow[],
  participantHumanIds: Array<string | null | undefined>,
  selfHumanId: string | null | undefined,
): boolean {
  const othersPresent = participantHumanIds.some(
    (humanId) => Boolean(humanId) && humanId !== selfHumanId,
  );
  return othersPresent && !transcriptHasSpeakerSeparation(rows);
}

/**
 * Alle Wort-Kennungen, die eine Handzuordnung abdeckt -- abgebildet wie die
 * Zuordnung selbst (`render-transcript.ts`, `normalizeSpeakerHint`): Bereich
 * "segment" = genau diese Woerter, Bereich "speaker" = alle Woerter dieses
 * Kanals (und Anbieter-Sprechers), alte Hinweise ohne Bereich = Kanal bzw.
 * Kanal und Sprecher des Ankerworts.
 */
export function manuallyAssignedWordIds(rows: TranscriptRow[]): Set<string> {
  const covered = new Set<string>();

  for (const row of rows) {
    const keys = new Map<string, { channel: number; speaker: number | null }>();
    for (const word of row.words ?? []) {
      if (typeof word.id !== "string") {
        continue;
      }
      keys.set(word.id, {
        channel: typeof word.channel === "number" ? word.channel : 0,
        speaker: null,
      });
    }
    for (const hint of row.speaker_hints ?? []) {
      if (hint.type !== "provider_speaker_index") {
        continue;
      }
      const value = parseHintValue(hint.value);
      const key =
        typeof hint.word_id === "string" ? keys.get(hint.word_id) : undefined;
      if (key && typeof value?.speaker_index === "number") {
        key.speaker = value.speaker_index;
        if (typeof value.channel === "number") {
          key.channel = value.channel;
        }
      }
    }

    const coverMatching = (channel: number, speaker?: number | null) => {
      for (const [id, key] of keys) {
        if (
          key.channel === channel &&
          (speaker === undefined || key.speaker === speaker)
        ) {
          covered.add(id);
        }
      }
    };

    for (const hint of row.speaker_hints ?? []) {
      if (hint.type !== "user_speaker_assignment") {
        continue;
      }
      const value = parseHintValue(hint.value);
      if (typeof value?.human_id !== "string") {
        continue;
      }
      if (value.scope === "segment" && Array.isArray(value.word_ids)) {
        const ids = value.word_ids.filter(
          (id): id is string => typeof id === "string" && id.length > 0,
        );
        if (ids.length > 0) {
          ids.forEach((id) => covered.add(id));
          continue;
        }
      }
      if (value.scope === "speaker" && typeof value.channel === "number") {
        coverMatching(
          value.channel,
          typeof value.speaker_index === "number"
            ? value.speaker_index
            : undefined,
        );
        continue;
      }
      const anchor =
        typeof hint.word_id === "string" ? keys.get(hint.word_id) : undefined;
      if (anchor) {
        coverMatching(
          anchor.channel,
          anchor.speaker === null ? undefined : anchor.speaker,
        );
      }
    }
  }

  return covered;
}

/**
 * Das Etikett, das ein Block vor dem Modell tragen darf. Ohne Trennung (siehe
 * `shouldNeutralizeSpeakerLabels`) heisst er `UNSEPARATED_SPEAKER_LABEL`,
 * ausser ein Mensch hat ihn von Hand zugeordnet.
 */
export function createSpeakerLabelResolver(
  rows: TranscriptRow[],
  participantHumanIds: Array<string | null | undefined>,
  selfHumanId: string | null | undefined,
): (segment: {
  speaker_label: string;
  words?: Array<{ id?: string | null }> | null;
}) => string {
  if (!shouldNeutralizeSpeakerLabels(rows, participantHumanIds, selfHumanId)) {
    return (segment) => segment.speaker_label;
  }
  const manual = manuallyAssignedWordIds(rows);
  return (segment) =>
    (segment.words ?? []).some(
      (word) => typeof word.id === "string" && manual.has(word.id),
    )
      ? segment.speaker_label
      : UNSEPARATED_SPEAKER_LABEL;
}
