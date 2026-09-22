import type {
  ChannelProfile as BoundChannelProfile,
  LiveTranscriptSegment,
  RenderTranscriptRequest,
  RenderedTranscriptSegment,
  SegmentKey as BoundSegmentKey,
  SegmentWord as BoundSegmentWord,
} from "@anlg/plugin-transcription";

import type { TranscriptWordMetadata } from "~/stt/timing";

export enum ChannelProfile {
  DirectMic = 0,
  RemoteParty = 1,
  MixedCapture = 2,
}

export type WordLike = {
  text: string;
  start_ms: number;
  end_ms: number;
  channel: ChannelProfile;
  metadata?: TranscriptWordMetadata | null;
};

export type PartialWord = WordLike;

type SpeakerHintData =
  | {
      type: "provider_speaker_index";
      speaker_index: number;
      provider?: string;
      channel?: number;
    }
  | { type: "user_speaker_assignment"; human_id: string };

export type RuntimeSpeakerHint = {
  wordIndex: number;
  data: SpeakerHintData;
};

export type RenderLabelContext = {
  getSelfHumanId: () => string | undefined;
  getHumanName: (id: string) => string | undefined;
  getParticipantHumanIds?: () => string[];
  /**
   * Kanaele, auf denen die Sprechertrennung MEHR ALS EINEN Sprecher gefunden
   * hat. Zwilling zu `SpeakerLabelContext::multi_speaker_channels` in Rust
   * (crates/transcript/src/label.rs).
   *
   * Fork (08.09.2026). Bewusst PFLICHT und nicht optional: ein fehlendes Feld
   * waere hier die Behauptung "kein Kanal traegt mehrere Sprecher", also genau
   * der Zustand, aus dem der Fehler kam -- und der Typecheck haette es
   * durchgelassen. So muss jeder Aufrufer die Frage beantworten.
   * `EMPTY_MULTI_SPEAKER_CHANNELS` ist die ausdrueckliche Antwort "keiner".
   */
  getMultiSpeakerChannels: () => ReadonlySet<SegmentChannelProfile>;
};

export const EMPTY_MULTI_SPEAKER_CHANNELS: ReadonlySet<SegmentChannelProfile> =
  new Set<SegmentChannelProfile>();

/**
 * Auf welchen Kanaelen tragen die Bloecke mehrere verschiedene Sprecher?
 *
 * Fork (08.09.2026). **Zweite Wahl** -- wo eine `RenderTranscriptRequest`
 * vorliegt, gehoert `multiSpeakerChannelsFromRequest` genommen: die zaehlt
 * ueber dieselben WOERTER wie Rust und kann deshalb nicht anders urteilen.
 *
 * Diese Fassung zaehlt ueber die BLOCK-Schluessel und ist damit eine Ebene
 * spaeter dran. Der Unterschied ist nicht theoretisch: `render.rs` wirft per
 * `retain(ist_sichtbar)` Bloecke weg, deren Woerter nur Leerraum tragen -- in
 * der Wortzaehlung sind sie enthalten, in der Blockliste nicht mehr. Bleiben
 * zwei Sprecher und faellt einer weg, kippt diese Zaehlung auf "einer" und die
 * Anzeige schreibt wieder den Namen des Nutzers hin.
 *
 * Trotzdem noetig: die Live-Ansicht laeuft VOR Rust und hat keine Anfrage,
 * und `WordLike` traegt gar kein Sprecherfeld (der Sprecher reist dort als
 * `speaker_hint` neben den Woertern). Fuer sie ist der Block-Schluessel die
 * einzige Quelle.
 */
/**
 * Auf welchen Kanaelen tragen die WOERTER der Anfrage mehrere Sprecher?
 *
 * Fork (08.09.2026). Exakter Zwilling von `render.rs::multi_speaker_channels`:
 * dieselbe Quelle (die Woerter der Anfrage), dieselbe Regel (mehr als ein
 * Sprecher-Index je Kanal). Sie koennen nicht auseinanderlaufen, weil sie
 * dasselbe zaehlen -- und genau darauf kommt es an: Rust entscheidet damit die
 * ZUWEISUNG, die Anzeige das ETIKETT, und ein Widerspruch zwischen beiden
 * schreibt dem Nutzer fremde Woerter zu.
 *
 * Diese Fassung ist ueberall dort die richtige, wo eine Anfrage vorliegt.
 */
export function multiSpeakerChannelsFromRequest(
  request: RenderTranscriptRequest | null | undefined,
): ReadonlySet<SegmentChannelProfile> {
  if (!request) {
    return EMPTY_MULTI_SPEAKER_CHANNELS;
  }

  const speakersByChannel = new Map<SegmentChannelProfile, Set<number>>();
  for (const transcript of request.transcripts) {
    for (const word of transcript.words) {
      if (typeof word.speaker_index !== "number") {
        continue;
      }
      const channel = channelProfileFromIndex(word.channel);
      let speakers = speakersByChannel.get(channel);
      if (!speakers) {
        speakers = new Set<number>();
        speakersByChannel.set(channel, speakers);
      }
      speakers.add(word.speaker_index);
    }
  }

  const multi = new Set<SegmentChannelProfile>();
  for (const [channel, speakers] of speakersByChannel) {
    if (speakers.size > 1) {
      multi.add(channel);
    }
  }
  return multi;
}

function channelProfileFromIndex(channel: number): SegmentChannelProfile {
  return channel === 0
    ? "DirectMic"
    : channel === 1
      ? "RemoteParty"
      : "MixedCapture";
}

export function multiSpeakerChannelsFromSegments(
  segments: ReadonlyArray<{ key: SegmentKey }>,
): ReadonlySet<SegmentChannelProfile> {
  const speakersByChannel = new Map<SegmentChannelProfile, Set<number>>();
  for (const segment of segments) {
    const speakerIndex = segment.key.speaker_index;
    if (typeof speakerIndex !== "number") {
      continue;
    }
    let speakers = speakersByChannel.get(segment.key.channel);
    if (!speakers) {
      speakers = new Set<number>();
      speakersByChannel.set(segment.key.channel, speakers);
    }
    speakers.add(speakerIndex);
  }

  const multi = new Set<SegmentChannelProfile>();
  for (const [channel, speakers] of speakersByChannel) {
    if (speakers.size > 1) {
      multi.add(channel);
    }
  }
  return multi;
}

export type SegmentKey = BoundSegmentKey;
export type SegmentWord = BoundSegmentWord & {
  metadata?: TranscriptWordMetadata | null;
};
type SegmentWithWordMetadata<T extends { words: BoundSegmentWord[] }> = Omit<
  T,
  "words"
> & {
  words: SegmentWord[];
};
export type Segment =
  | SegmentWithWordMetadata<LiveTranscriptSegment>
  | SegmentWithWordMetadata<RenderedTranscriptSegment>;
export type SegmentChannelProfile = BoundChannelProfile;

// Wie weit die Mitten zweier gleich geschriebener Woerter auseinander liegen
// duerfen, damit sie noch als DASSELBE gesprochene Wort gelten. Fork
// (02.09.2026): der Nachbearbeitungsweg liefert seit den gemessenen Wortzeiten
// andere -- genauere -- Zahlen als der Live-Weg, der weiter interpoliert. Zwei
// Sekunden decken die beobachtete Drift ab und bleiben deutlich unter dem
// Abstand, in dem ein Sprecher dasselbe Wort ein zweites Mal sagt.
const REPLACEMENT_TEXT_MATCH_WINDOW_MS = 2_000;
const REPLACEMENT_MIN_OVERLAP_RATIO = 0.8;

/**
 * Vergibt fortlaufende Nummern an Sprecher ohne Namen. Zwillingsstueck zu
 * `SpeakerLabeler` in `crates/transcript/src/label.rs` -- beide Seiten muessen
 * dasselbe tun, sonst zeigt die Oberflaeche etwas anderes als der Spiegel.
 *
 * Fork (21.09.2026): der geerbte Deckel auf die Teilnehmerzahl ist RAUS. Er
 * legte per `Math.min` jeden ueberzaehligen Schluessel auf dieselbe letzte
 * erlaubte Nummer und verschmolz damit in einer 122,8-Minuten-Raumaufnahme mit
 * vier Personen die beiden Hauptredner zu einem Etikett. Ein Etikett zu viel
 * kann ein Mensch zusammenfuehren, zwei verschmolzene Menschen nicht mehr.
 * Volle Begruendung im Rust-Zwilling.
 */
export class SpeakerLabelManager {
  private unknownSpeakerMap: Map<string, number> = new Map();
  private nextIndex = 1;

  getUnknownSpeakerNumber(key: SegmentKey): number {
    const serialized = SegmentKeyUtils.serialize(key);
    const existing = this.unknownSpeakerMap.get(serialized);
    if (existing !== undefined) {
      return existing;
    }

    // Jeder neue Schluessel bekommt eine eigene Nummer. Kein Deckel.
    const newIndex = this.nextIndex;
    this.unknownSpeakerMap.set(serialized, newIndex);
    this.nextIndex += 1;
    return newIndex;
  }

  static fromSegments(
    segments: Segment[],
    ctx?: RenderLabelContext,
  ): SpeakerLabelManager {
    const manager = new SpeakerLabelManager();
    for (const segment of segments) {
      if (!SegmentKeyUtils.isKnownSpeaker(segment.key, ctx)) {
        manager.getUnknownSpeakerNumber(segment.key);
      }
    }
    return manager;
  }
}

export const SegmentKeyUtils = {
  serialize: (key: SegmentKey): string => {
    return JSON.stringify([
      key.channel,
      key.speaker_index ?? null,
      key.speaker_human_id ?? null,
    ]);
  },

  isKnownSpeaker: (key: SegmentKey, ctx?: RenderLabelContext): boolean => {
    if (key.speaker_human_id) {
      return true;
    }

    // Fork (08.09.2026): der Mehrsprecher-Vorbehalt. Ohne ihn galt auf einer
    // Raumaufnahme JEDER Mikrofon-Block als bekannter Sprecher und bekam den
    // Namen des Nutzers -- vier Menschen, ein Name. Muss deckungsgleich mit
    // `renderLabel` unten bleiben und mit `is_known_speaker` in Rust.
    if (
      ctx &&
      key.channel === "DirectMic" &&
      !ctx.getMultiSpeakerChannels().has(key.channel)
    ) {
      return Boolean(ctx.getSelfHumanId());
    }

    if (ctx && key.channel === "RemoteParty") {
      return Boolean(getUniqueRemoteParticipantHumanId(ctx));
    }

    return false;
  },

  renderLabel: (
    key: SegmentKey,
    ctx?: RenderLabelContext,
    manager?: SpeakerLabelManager,
  ): string => {
    const assignedHumanId = key.speaker_human_id;

    if (ctx && assignedHumanId != null) {
      const human = ctx.getHumanName(assignedHumanId);
      if (human) {
        return human;
      }
    }

    // Fork (08.09.2026): siehe `isKnownSpeaker`. Diese Datei rechnet das
    // Etikett SELBST aus (transcript.tsx nimmt nicht Rusts `speaker_label`) --
    // der Fix in Rust allein aendert die Anzeige deshalb nicht.
    if (
      ctx &&
      key.channel === "DirectMic" &&
      assignedHumanId == null &&
      !ctx.getMultiSpeakerChannels().has(key.channel)
    ) {
      const selfHumanId = ctx.getSelfHumanId();
      if (selfHumanId) {
        const selfHuman = ctx.getHumanName(selfHumanId);
        return selfHuman || "You";
      }
    }

    if (ctx && key.channel === "RemoteParty" && assignedHumanId == null) {
      const remoteHumanId = getUniqueRemoteParticipantHumanId(ctx);
      if (remoteHumanId) {
        return ctx.getHumanName(remoteHumanId) || remoteHumanId;
      }
    }

    if (manager) {
      const speakerNumber = manager.getUnknownSpeakerNumber(key);
      return `Speaker ${speakerNumber}`;
    }

    const channelLabel =
      key.channel === "DirectMic"
        ? "A"
        : key.channel === "RemoteParty"
          ? "B"
          : "C";

    return key.speaker_index !== null && key.speaker_index !== undefined
      ? `Speaker ${key.speaker_index + 1}`
      : `Speaker ${channelLabel}`;
  },
};

function getUniqueRemoteParticipantHumanId(
  ctx: RenderLabelContext,
): string | undefined {
  const selfHumanId = ctx.getSelfHumanId();
  const participantHumanIds = ctx.getParticipantHumanIds?.() ?? [];
  const remoteHumanIds = [
    ...new Set(
      participantHumanIds.filter(
        (humanId) => humanId && humanId !== selfHumanId,
      ),
    ),
  ];

  return remoteHumanIds.length === 1 ? remoteHumanIds[0] : undefined;
}

export function mergeRenderedAndLiveSegments(
  renderedSegments: Segment[],
  liveSegments: Segment[],
  currentRequest?: RenderTranscriptRequest | null,
): Segment[] {
  const liveSegmentIds = new Set(
    liveSegments
      .map((segment) => segment.id)
      .filter((id): id is string => typeof id === "string" && id.length > 0),
  );
  const liveWordIds = new Set(
    liveSegments.flatMap((segment) =>
      segment.words
        .map((word) => word.id)
        .filter((id): id is string => typeof id === "string" && id.length > 0),
    ),
  );
  const currentWordIds = currentRequest
    ? new Set(
        currentRequest.transcripts.flatMap((transcript) =>
          transcript.words.map((word) => word.id),
        ),
      )
    : null;
  const survivingRenderedSegments = renderedSegments.filter(
    (segment) => !(segment.id && liveSegmentIds.has(segment.id)),
  );
  // Ein persistiertes Wort faellt raus, wenn es entweder ohnehin schon aus dem
  // Live-Weg oder aus SQLite verschwunden ist -- oder wenn ein noch nicht
  // persistiertes Live-Wort es ersetzt. Wer WEN ersetzt, entscheidet
  // `matchPendingLiveReplacements` fuer alle Woerter zusammen, nicht Wort fuer
  // Wort: nur so bleibt eine echte Wortwiederholung erhalten.
  const isDroppedElsewhere = (word: SegmentWord): boolean =>
    Boolean(
      (word.id && liveWordIds.has(word.id)) ||
      (word.id && currentWordIds && !currentWordIds.has(word.id)),
    );
  const replacedRenderedWordIds = matchPendingLiveReplacements(
    survivingRenderedSegments,
    liveSegments,
    currentWordIds,
    isDroppedElsewhere,
  );
  const renderedOnlySegments = survivingRenderedSegments.flatMap((segment) =>
    filterSegmentWords(
      segment,
      (word) =>
        !(
          isDroppedElsewhere(word) ||
          (word.id != null && replacedRenderedWordIds.has(word.id))
        ),
      "persisted",
    ),
  );

  return [...renderedOnlySegments, ...liveSegments].sort(
    (a, b) => a.start_ms - b.start_ms || a.end_ms - b.end_ms,
  );
}

export function applyRenderRequestIdentitiesToSegments(
  segments: Segment[],
  request?: RenderTranscriptRequest | null,
): Segment[] {
  if (!request || segments.length === 0) {
    return segments;
  }

  const assignments = request.transcripts.flatMap(
    (transcript) => transcript.assignments,
  );
  if (assignments.length === 0) {
    return segments;
  }

  const completeChannels = getCompleteChannels(request);
  // Aus der Anfrage, nicht aus den Bloecken: dies ist der TypeScript-Zwilling
  // von `apply_identity_rules`, und der urteilt in Rust ueber genau diese
  // Woerter. Zwei Zaehlungen ueber verschiedene Ebenen wuerden hier eine
  // Zuweisung setzen, die das Etikett danach anders bewertet.
  const multiSpeakerChannels = multiSpeakerChannelsFromRequest(request);
  const assignmentsByWordId = new Map<string, string>();
  const assignmentsByChannel = new Map<SegmentChannelProfile, string>();
  const assignmentsByChannelSpeaker = new Map<string, string>();

  for (const assignment of assignments) {
    if (!assignment.human_id) {
      continue;
    }

    if (assignment.scope.kind === "words") {
      for (const wordId of assignment.scope.word_ids) {
        assignmentsByWordId.set(wordId, assignment.human_id);
      }
    } else if (assignment.scope.kind === "channel_speaker") {
      assignmentsByChannelSpeaker.set(
        channelSpeakerKey(
          assignment.scope.channel,
          assignment.scope.speaker_index,
        ),
        assignment.human_id,
      );
    } else if (
      completeChannels.has(assignment.scope.channel) &&
      // Fork (08.09.2026): Zwilling zu `apply_identity_rules` in Rust
      // (crates/transcript/src/segments/speakers.rs). Ohne diesen Vorbehalt
      // schrieb die Live-Ansicht den Selbst-Menschen auf Bloecke zurueck, die
      // Rust bewusst unzugeordnet gelassen hatte -- der Fix waere waehrend der
      // laufenden Aufnahme wirkungslos gewesen.
      !multiSpeakerChannels.has(assignment.scope.channel)
    ) {
      assignmentsByChannel.set(assignment.scope.channel, assignment.human_id);
    }
  }

  return segments.flatMap((segment) => {
    const scopedHumanId =
      (typeof segment.key.speaker_index === "number"
        ? assignmentsByChannelSpeaker.get(
            channelSpeakerKey(segment.key.channel, segment.key.speaker_index),
          )
        : undefined) ??
      segment.key.speaker_human_id ??
      assignmentsByChannel.get(segment.key.channel) ??
      null;
    const runs: Array<{ humanId: string | null; words: SegmentWord[] }> = [];
    let previousHumanId = scopedHumanId;

    for (const word of segment.words) {
      let humanId =
        (word.id ? assignmentsByWordId.get(word.id) : undefined) ??
        scopedHumanId;
      if (!word.id && !word.is_final) {
        humanId = previousHumanId;
      }

      const run = runs[runs.length - 1];
      if (run?.humanId === humanId) {
        run.words.push(word);
      } else {
        runs.push({ humanId, words: [word] });
      }
      previousHumanId = humanId;
    }

    if (runs.length === 0) {
      return [segment];
    }

    if (runs.length === 1) {
      const humanId = runs[0]!.humanId;
      if ((segment.key.speaker_human_id ?? null) === humanId) {
        return [segment];
      }

      return [
        {
          ...segment,
          key: { ...segment.key, speaker_human_id: humanId },
        },
      ];
    }

    return runs.map(({ humanId, words }) => ({
      ...createSegmentFragment(
        segment,
        words,
        `identity:${humanId ?? "unassigned"}`,
      ),
      key: { ...segment.key, speaker_human_id: humanId },
    }));
  });
}

function getCompleteChannels(
  request: RenderTranscriptRequest,
): Set<SegmentChannelProfile> {
  const participantHumanIds = new Set(
    request.participant_human_ids.filter(Boolean),
  );
  if (request.self_human_id) {
    participantHumanIds.add(request.self_human_id);
  }

  return new Set([
    "DirectMic",
    ...(participantHumanIds.size === 2 ? (["RemoteParty"] as const) : []),
  ]);
}

function channelSpeakerKey(
  channel: SegmentChannelProfile,
  speakerIndex: number,
): string {
  return `${channel}:${speakerIndex}`;
}

/// Welche persistierten Woerter ersetzt der Live-Weg gerade?
///
/// Fork (02.09.2026), der Grund fuer den Umbau: bis hierher entschied das
/// allein die zeitliche Ueberlappung -- ein persistiertes Wort galt ab 80 %
/// Ueberlappung mit einem noch nicht persistierten Live-Wort als ersetzt.
/// Solange BEIDE Seiten ihre Wortzeiten gleichmaessig ueber den Abschnitt
/// verteilten, war das verlaesslich. Seit der Nachbearbeitungsweg GEMESSENE
/// Zeiten liefert und der Live-Weg weiter interpoliert, weichen sie um mehr als
/// 20 % der kuerzeren Wortdauer voneinander ab -- und dasselbe Wort stand
/// zweimal auf dem Schirm.
///
/// Die naheliegende Antwort waere eine tolerantere Schwelle. Sie ist die
/// schlechteste: irgendwann verschluckt sie ein echtes zweites Vorkommen
/// desselben Wortes, und das ist stiller Datenverlust. Deshalb steht die
/// Zuordnung jetzt auf vier Beinen statt auf einem:
///
/// 1. **Kanal und Sprecher** trennen die Woerter in Toepfe.
/// 2. **Ueberlappung** bleibt das starke Kriterium, unveraendert bei 80 %.
/// 3. **Text** traegt, wo die Zeiten auseinanderlaufen -- gleiche Schreibweise
///    und die Mitten hoechstens [`REPLACEMENT_TEXT_MATCH_WINDOW_MS`]
///    auseinander.
/// 4. **Reihenfolge und Eins-zu-eins:** die Zuordnung laeuft monoton mit, und
///    jedes persistierte Wort wird hoechstens einmal beansprucht. Sagt jemand
///    "ja ja", nimmt das erste Live-"ja" das erste persistierte und das zweite
///    das zweite. Ein einzelnes Live-"ja" raeumt nie beide weg.
///
/// Fehlerrichtung im Zweifel: **stehen lassen.** Ein doppeltes Wort sieht
/// kaputt aus, ein verschlucktes ist weg. Deshalb ist das Textkriterium an ein
/// Zeitfenster gebunden, statt allein auf der Schreibweise zu stehen -- wer
/// unsicher ist, ersetzt nicht.
function matchPendingLiveReplacements(
  renderedSegments: Segment[],
  liveSegments: Segment[],
  currentWordIds: Set<string> | null,
  isDroppedElsewhere: (word: SegmentWord) => boolean,
): Set<string> {
  const claimed = new Set<string>();
  if (!currentWordIds) {
    return claimed;
  }

  const pendingByKey = groupWordsByKey(liveSegments, (word) =>
    Boolean(word.id && !currentWordIds.has(word.id)),
  );
  if (pendingByKey.size === 0) {
    return claimed;
  }

  const renderedByKey = groupWordsByKey(
    renderedSegments,
    (word) => Boolean(word.id) && !isDroppedElsewhere(word),
    pendingByKey,
  );

  for (const [key, pendingWords] of pendingByKey) {
    const renderedWords = renderedByKey.get(key);
    if (!renderedWords) {
      continue;
    }

    pendingWords.sort(byStartThenEnd);
    renderedWords.sort(byStartThenEnd);

    let nextRenderedIndex = 0;
    for (const liveWord of pendingWords) {
      for (
        let index = nextRenderedIndex;
        index < renderedWords.length;
        index += 1
      ) {
        const renderedWord = renderedWords[index]!;
        // Weiter als das Fenster reicht, kann kein Treffer mehr liegen -- und
        // weil die Liste sortiert ist, auch bei keinem spaeteren Kandidaten.
        if (
          renderedWord.start_ms >
          liveWord.end_ms + REPLACEMENT_TEXT_MATCH_WINDOW_MS
        ) {
          break;
        }
        if (isSameSpokenWord(renderedWord, liveWord)) {
          claimed.add(renderedWord.id!);
          nextRenderedIndex = index + 1;
          break;
        }
      }
    }
  }

  return claimed;
}

function groupWordsByKey(
  segments: Segment[],
  keepWord: (word: SegmentWord) => boolean,
  onlyKeys?: Map<string, SegmentWord[]>,
): Map<string, SegmentWord[]> {
  const grouped = new Map<string, SegmentWord[]>();
  for (const segment of segments) {
    const key = SegmentKeyUtils.serialize(segment.key);
    if (onlyKeys && !onlyKeys.has(key)) {
      continue;
    }

    for (const word of segment.words) {
      // Ein Wort ohne Dauer hat keine Zeitachse, an der sich irgendetwas
      // festmachen liesse -- das war schon vorher so und bleibt so.
      if (word.end_ms <= word.start_ms || !keepWord(word)) {
        continue;
      }

      const words = grouped.get(key);
      if (words) {
        words.push(word);
      } else {
        grouped.set(key, [word]);
      }
    }
  }
  return grouped;
}

function byStartThenEnd(left: SegmentWord, right: SegmentWord): number {
  return left.start_ms - right.start_ms || left.end_ms - right.end_ms;
}

function isSameSpokenWord(
  renderedWord: SegmentWord,
  liveWord: SegmentWord,
): boolean {
  const overlap =
    Math.min(renderedWord.end_ms, liveWord.end_ms) -
    Math.max(renderedWord.start_ms, liveWord.start_ms);
  const shorterDuration = Math.min(
    renderedWord.end_ms - renderedWord.start_ms,
    liveWord.end_ms - liveWord.start_ms,
  );
  if (
    overlap > 0 &&
    overlap / shorterDuration >= REPLACEMENT_MIN_OVERLAP_RATIO
  ) {
    return true;
  }

  const renderedText = normalizeWordText(renderedWord.text);
  if (
    renderedText.length === 0 ||
    renderedText !== normalizeWordText(liveWord.text)
  ) {
    return false;
  }

  const midpointDistance = Math.abs(
    (renderedWord.start_ms + renderedWord.end_ms) / 2 -
      (liveWord.start_ms + liveWord.end_ms) / 2,
  );
  return midpointDistance <= REPLACEMENT_TEXT_MATCH_WINDOW_MS;
}

/// Vergleicht die Schreibweise ohne alles, was der eine Weg setzt und der
/// andere nicht: Leerraum, Satzzeichen, Gross-/Kleinschreibung.
function normalizeWordText(text: string): string {
  return text
    .normalize("NFC")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]/gu, "");
}

function filterSegmentWords(
  segment: Segment,
  keepWord: (word: SegmentWord) => boolean,
  fragmentKind: string,
): Segment[] {
  if (segment.words.every(keepWord)) {
    return [segment];
  }

  const fragments: Segment[] = [];
  let words: SegmentWord[] = [];
  const flushFragment = () => {
    if (words.length > 0) {
      fragments.push(createSegmentFragment(segment, words, fragmentKind));
      words = [];
    }
  };

  for (const word of segment.words) {
    if (keepWord(word)) {
      words.push(word);
    } else {
      flushFragment();
    }
  }
  flushFragment();
  return fragments;
}

function createSegmentFragment(
  segment: Segment,
  words: SegmentWord[],
  fragmentKind: string,
): Segment {
  const first = words[0]!;
  const last = words[words.length - 1]!;
  return {
    ...segment,
    id: [
      segment.id || "segment",
      fragmentKind,
      first.id ?? first.start_ms,
      last.id ?? last.end_ms,
    ].join(":"),
    start_ms: first.start_ms,
    end_ms: last.end_ms,
    text: words
      .map((word) => word.text)
      .join("")
      .trim(),
    words,
  };
}
