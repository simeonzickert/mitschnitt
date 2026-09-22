import { describe, expect, it, vi } from "vitest";

import type {
  IdentityAssignment,
  RenderTranscriptRequest,
} from "@anlg/plugin-transcription";

import {
  applyRenderRequestIdentitiesToSegments,
  mergeRenderedAndLiveSegments,
  SegmentKeyUtils,
  SpeakerLabelManager,
  EMPTY_MULTI_SPEAKER_CHANNELS,
  multiSpeakerChannelsFromRequest,
  type RenderLabelContext,
  type Segment,
} from "./live-segment";

const ctx: RenderLabelContext = {
  getSelfHumanId: () => "self",
  getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
  getHumanName: (id) => (id === "self" ? "Me" : undefined),
};
const twoPersonCtx: RenderLabelContext = {
  getSelfHumanId: () => "self",
  getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
  getHumanName: (id) =>
    id === "self" ? "Me" : id === "remote" ? "Artem" : undefined,
  getParticipantHumanIds: () => ["self", "remote"],
};

describe("Raumaufnahme: mehrere Sprecher auf einem Kanal", () => {
  const vierSprecherCtx: RenderLabelContext = {
    getSelfHumanId: () => "self",
    getHumanName: (id) => (id === "self" ? "Mads Verlin" : undefined),
    getParticipantHumanIds: () => ["self", "g1", "g2", "g3"],
    getMultiSpeakerChannels: () => new Set(["DirectMic" as const]),
  };

  const bloecke = (indizes: number[]): Segment[] =>
    indizes.map(
      (speakerIndex) =>
        ({
          id: `s${speakerIndex}`,
          key: {
            channel: "DirectMic",
            speaker_index: speakerIndex,
            speaker_human_id: null,
          },
          start_ms: speakerIndex * 1000,
          end_ms: speakerIndex * 1000 + 400,
          text: "x",
          // Mindestens EIN Wort ist Pflicht:
          // `applyRenderRequestIdentitiesToSegments` bildet seine Laeufe aus
          // den Woertern und gibt einen wortlosen Block unveraendert zurueck.
          // Ohne dieses Wort waren beide Zuweisungs-Tests still gruen.
          words: [
            {
              text: "x",
              start_ms: speakerIndex * 1000,
              end_ms: speakerIndex * 1000 + 400,
              channel: 0,
              is_final: true,
              id: `w${speakerIndex}`,
            },
          ],
        }) as unknown as Segment,
    );

  // Der Befund vom 08.09.2026: eine Zwei-Stunden-Raumaufnahme mit vier
  // Personen zeigte viermal seinen eigenen Namen. Der Rust-Renderer war schon
  // repariert -- diese Ansicht rechnet das Etikett aber SELBST aus
  // (transcript.tsx nimmt `speaker_label` nicht), also lebte der Fehler hier
  // weiter. Genau das prueft dieser Test.
  it("gibt vier getrennten Sprechern nicht viermal den Namen des Nutzers", () => {
    const manager = SpeakerLabelManager.fromSegments(
      bloecke([0, 1, 2, 3]),
      vierSprecherCtx,
    );
    const etiketten = bloecke([0, 1, 2, 3]).map((segment) =>
      SegmentKeyUtils.renderLabel(segment.key, vierSprecherCtx, manager),
    );

    expect(etiketten).not.toContain("Mads Verlin");
    expect(new Set(etiketten).size).toBe(4);
  });

  // Der Befund vom 21.09.2026: in einer 122,8-Minuten-Raumaufnahme mit vier
  // Personen fand die Trennung FUENF Stimmgruppen. Der Teilnehmer-Deckel warf
  // die fuenfte auf dasselbe Etikett wie die vierte -- und traf damit die
  // beiden Hauptredner.
  it("gibt fuenf Stimmgruppen bei vier Teilnehmern fuenf Etiketten", () => {
    const manager = SpeakerLabelManager.fromSegments(
      bloecke([0, 1, 2, 3, 4]),
      vierSprecherCtx,
    );
    const etiketten = bloecke([0, 1, 2, 3, 4]).map((segment) =>
      SegmentKeyUtils.renderLabel(segment.key, vierSprecherCtx, manager),
    );

    expect(new Set(etiketten).size).toBe(5);
  });

  // Der Labeler vergibt seine Nummern nach `isKnownSpeaker`, das Etikett liest
  // nach `renderLabel`. Laufen die beiden auseinander, faellt ein Block durch
  // und bekommt eine Nummer, die niemand reserviert hat. Der Test misst
  // deshalb BEIDE Seiten -- eine erste Fassung prueft nur `isKnownSpeaker`
  // und waere gruen geblieben, waere der Vorbehalt aus `renderLabel` gefallen.
  it("haelt isKnownSpeaker und renderLabel deckungsgleich", () => {
    for (const segment of bloecke([0, 1, 2, 3])) {
      expect(SegmentKeyUtils.isKnownSpeaker(segment.key, vierSprecherCtx)).toBe(
        false,
      );
      expect(SegmentKeyUtils.renderLabel(segment.key, vierSprecherCtx)).not.toBe(
        "Mads Verlin",
      );
    }
  });

  // Die Verdrahtung selbst, nicht nur die Funktion: hier wird die Menge so
  // berechnet, wie `transcript.tsx` sie berechnet -- aus der Anfrage. Ohne
  // diesen Test waere ein falsch geschnittener Aufruf dort unbemerkt
  // geblieben, waehrend alle Tests daneben gruen bleiben. Genau diese Klasse
  // (Funktion richtig, Verdrahtung falsch) war der teuerste Befund des Tages.
  it("erkennt die vier Sprecher auch aus der Anfrage, nicht nur aus den Bloecken", () => {
    const request = {
      transcripts: [
        {
          started_at: 0,
          words: [0, 1, 2, 3].map((speakerIndex) => ({
            id: `w${speakerIndex}`,
            text: "x",
            start_ms: speakerIndex * 1000,
            end_ms: speakerIndex * 1000 + 400,
            channel: 0,
            speaker_index: speakerIndex,
          })),
          assignments: [],
        },
      ],
      participant_human_ids: ["self", "g1", "g2", "g3"],
      self_human_id: "self",
      humans: [],
    } as unknown as RenderTranscriptRequest;

    const ausAnfrage = multiSpeakerChannelsFromRequest(request);
    expect(ausAnfrage.has("DirectMic")).toBe(true);

    const ctxAusAnfrage: RenderLabelContext = {
      ...vierSprecherCtx,
      getMultiSpeakerChannels: () => ausAnfrage,
    };
    expect(
      SegmentKeyUtils.renderLabel(bloecke([2])[0]!.key, ctxAusAnfrage),
    ).not.toBe("Mads Verlin");
  });

  // Gegenprobe: ein einzelner Sprecher in der Anfrage darf den Kanal NICHT
  // als Mehrsprecher-Kanal ausweisen, sonst verliert der Nutzer im Normalfall
  // seinen Namen.
  it("weist einen Kanal mit einem einzigen Sprecher nicht als Mehrsprecher aus", () => {
    const request = {
      transcripts: [
        {
          started_at: 0,
          words: [
            {
              id: "w0",
              text: "x",
              start_ms: 0,
              end_ms: 400,
              channel: 0,
              speaker_index: 0,
            },
          ],
          assignments: [],
        },
      ],
      participant_human_ids: ["self"],
      self_human_id: "self",
      humans: [],
    } as unknown as RenderTranscriptRequest;

    expect(multiSpeakerChannelsFromRequest(request).has("DirectMic")).toBe(
      false,
    );
  });

  // Nicht-Regression: ein einzelner Sprecher auf dem Mikrofonkanal ist
  // weiterhin der Nutzer. Das ist der Normalfall.
  it("laesst einen einzelnen Mikrofon-Sprecher der Nutzer bleiben", () => {
    const einSprecherCtx: RenderLabelContext = {
      ...vierSprecherCtx,
      getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
    };

    expect(
      SegmentKeyUtils.renderLabel(bloecke([0])[0]!.key, einSprecherCtx),
    ).toBe("Mads Verlin");
    expect(
      SegmentKeyUtils.isKnownSpeaker(bloecke([0])[0]!.key, einSprecherCtx),
    ).toBe(true);
  });

  // Der zweite Weg, auf dem der Nutzer zurueckgeschrieben wurde: waehrend der
  // laufenden Aufnahme legt die Live-Ansicht die Kanal-Zuweisungen der
  // Render-Anfrage selbst auf die Bloecke. Ohne Vorbehalt macht das den Fix
  // live wieder zunichte.
  it("schreibt die Kanal-Zuweisung nicht auf einen Kanal mit mehreren Sprechern", () => {
    const request = {
      // Die Kanal-Zuweisung, die der Rust-Renderer immer mitschickt
      // (`channel_assignments_for_participants`). Ohne sie steigt
      // `applyRenderRequestIdentitiesToSegments` frueh aus und der Test
      // prueft nichts -- genau so ist die erste Fassung dieses Tests still
      // gruen gewesen.
      transcripts: [
        {
          started_at: 0,
          // Die Woerter muessen mit: seit die Mehrsprecher-Frage aus der
          // ANFRAGE beantwortet wird (deckungsgleich mit Rust), ist eine
          // wortlose Anfrage die Aussage "kein Kanal traegt mehrere Sprecher".
          words: [0, 1, 2, 3].map((speakerIndex) => ({
            id: `w${speakerIndex}`,
            text: "x",
            start_ms: speakerIndex * 1000,
            end_ms: speakerIndex * 1000 + 400,
            channel: 0,
            speaker_index: speakerIndex,
          })),
          assignments: [
            {
              human_id: "self",
              scope: { kind: "channel", channel: "DirectMic" },
            } as unknown as IdentityAssignment,
          ],
        },
      ],
      participant_human_ids: ["self", "g1", "g2", "g3"],
      self_human_id: "self",
      humans: [],
    } as unknown as RenderTranscriptRequest;

    const ergebnis = applyRenderRequestIdentitiesToSegments(
      bloecke([0, 1, 2, 3]),
      request,
    );

    for (const segment of ergebnis) {
      expect(segment.key.speaker_human_id ?? null).toBeNull();
    }
  });

  it("schreibt die Kanal-Zuweisung weiterhin bei einem einzelnen Sprecher", () => {
    const request = {
      transcripts: [
        {
          started_at: 0,
          words: [
            {
              id: "w0",
              text: "x",
              start_ms: 0,
              end_ms: 400,
              channel: 0,
              speaker_index: 0,
            },
          ],
          assignments: [
            {
              human_id: "self",
              scope: { kind: "channel", channel: "DirectMic" },
            } as unknown as IdentityAssignment,
          ],
        },
      ],
      participant_human_ids: ["self"],
      self_human_id: "self",
      humans: [],
    } as unknown as RenderTranscriptRequest;

    const ergebnis = applyRenderRequestIdentitiesToSegments(
      bloecke([0]),
      request,
    );

    expect(ergebnis[0]!.key.speaker_human_id).toBe("self");
  });
});

describe("SegmentKeyUtils", () => {
  it("treats diarized direct-mic segments as self", () => {
    const key: Parameters<typeof SegmentKeyUtils.isKnownSpeaker>[0] = {
      channel: "DirectMic",
      speaker_index: 2,
      speaker_human_id: null,
    };

    expect(SegmentKeyUtils.isKnownSpeaker(key, ctx)).toBe(true);
    expect(SegmentKeyUtils.renderLabel(key, ctx)).toBe("Me");
  });

  it("does not label assigned direct-mic segments as self when the name is unavailable", () => {
    const key: Parameters<typeof SegmentKeyUtils.renderLabel>[0] = {
      channel: "DirectMic",
      speaker_index: 1,
      speaker_human_id: "remote",
    };

    expect(SegmentKeyUtils.renderLabel(key, ctx)).toBe("Speaker 2");
  });

  // Hiess vorher "caps unknown speaker labels when a participant max is
  // provided" und schrieb das Verschmelzen fest: der dritte Sprecher bekam bei
  // zwei Teilnehmern dasselbe Etikett wie der zweite. Umgeschrieben auf die
  // Zusage, die jetzt gilt (21.09.2026) -- mehr Stimmgruppen als Teilnehmer
  // sind ein Anzeigefall, nie ein Grund, zwei Menschen zusammenzulegen.
  it("legt nie zwei Sprecher-Schluessel auf dasselbe Etikett", () => {
    const segments: Segment[] = [0, 1, 2, 3, 4].map(
      (speakerIndex) =>
        ({
          id: `segment-${speakerIndex}`,
          key: {
            channel: "RemoteParty",
            speaker_index: speakerIndex,
            speaker_human_id: null,
          },
          words: [],
          start_ms: 0,
          end_ms: 0,
          text: "",
        }) as Segment,
    );
    const manager = SpeakerLabelManager.fromSegments(segments, undefined);
    const etiketten = segments.map((segment) =>
      SegmentKeyUtils.renderLabel(segment.key, undefined, manager),
    );

    expect(etiketten).toEqual([
      "Speaker 1",
      "Speaker 2",
      "Speaker 3",
      "Speaker 4",
      "Speaker 5",
    ]);
    expect(new Set(etiketten).size).toBe(5);
  });

  it("labels remote-party segments as the unique other participant", () => {
    const key: Parameters<typeof SegmentKeyUtils.renderLabel>[0] = {
      channel: "RemoteParty",
      speaker_index: 0,
      speaker_human_id: null,
    };

    expect(SegmentKeyUtils.isKnownSpeaker(key, twoPersonCtx)).toBe(true);
    expect(SegmentKeyUtils.renderLabel(key, twoPersonCtx)).toBe("Artem");
  });

  it("keeps a number rather than guessing when the name is not derivable", () => {
    const key: Parameters<typeof SegmentKeyUtils.renderLabel>[0] = {
      channel: "RemoteParty",
      speaker_index: 0,
      speaker_human_id: null,
    };

    // Two remotes: there is no single right answer, so never pick one.
    const twoRemotes: RenderLabelContext = {
      getSelfHumanId: () => "self",
      getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
      getHumanName: (id) =>
        id === "self" ? "Me" : id === "remote" ? "Artem" : "Tom",
      getParticipantHumanIds: () => ["self", "remote", "remote-2"],
    };
    // The owner stored twice used to look exactly like this. It must not.
    const ownerCopy: RenderLabelContext = {
      getSelfHumanId: () => "self",
      getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
      getHumanName: (id) => (id === "self" ? "Me" : "Artem"),
      getParticipantHumanIds: () => ["remote", "self-calendar-copy"],
    };
    // No calendar event at all, or an event without an attendee list.
    const noParticipants: RenderLabelContext = {
      getSelfHumanId: () => "self",
      getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
      getHumanName: () => undefined,
      getParticipantHumanIds: () => [],
    };
    const noParticipantSource: RenderLabelContext = {
      getSelfHumanId: () => "self",
      getMultiSpeakerChannels: () => EMPTY_MULTI_SPEAKER_CHANNELS,
      getHumanName: () => undefined,
    };

    for (const context of [
      twoRemotes,
      ownerCopy,
      noParticipants,
      noParticipantSource,
    ]) {
      expect(SegmentKeyUtils.isKnownSpeaker(key, context)).toBe(false);
      expect(SegmentKeyUtils.renderLabel(key, context)).toBe("Speaker 1");
    }
  });

  it("lets a manual assignment win over the derived participant", () => {
    const key: Parameters<typeof SegmentKeyUtils.renderLabel>[0] = {
      channel: "RemoteParty",
      speaker_index: 0,
      speaker_human_id: "assigned",
    };
    const assignedCtx: RenderLabelContext = {
      ...twoPersonCtx,
      getHumanName: (id) =>
        id === "assigned" ? "Tom" : id === "remote" ? "Artem" : "Me",
    };

    expect(SegmentKeyUtils.renderLabel(key, assignedCtx)).toBe("Tom");
  });

});

describe("mergeRenderedAndLiveSegments", () => {
  it("preserves persisted words that share a segment with the live tail", () => {
    const persisted = createSegment("persisted", [
      { id: "word-prefix", startMs: 0 },
      { id: "word-tail", startMs: 100 },
    ]);
    const live = createSegment("live", [{ id: "word-tail", startMs: 100 }]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live]);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-prefix", "word-tail"]);
  });

  it("keeps split persisted fragments in timestamp order", () => {
    const persisted = createSegment("persisted", [
      { id: "word-before", startMs: 0 },
      { id: "word-live", startMs: 100 },
      { id: "word-after", startMs: 200 },
    ]);
    const live = createSegment("live", [{ id: "word-live", startMs: 100 }]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live]);

    expect(merged.map((segment) => segment.words[0]?.id)).toEqual([
      "word-before",
      "word-live",
      "word-after",
    ]);
  });

  it("preserves the persisted prefix beside an id-less live partial", () => {
    const persisted = createSegment("persisted", [
      { id: "word-prefix", startMs: 0 },
    ]);
    const live = createSegment("live", [{ startMs: 100 }]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live]);

    expect(merged).toEqual([persisted, live]);
  });

  it("replaces a frozen word before its new id reaches SQLite", () => {
    const persisted = createSegment("persisted", [
      { id: "word-old", startMs: 100 },
    ]);
    const live = createSegment("live", [
      { id: "word-replacement", startMs: 100 },
    ]);
    const request = createRequest(["word-old"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-replacement"]);
  });

  it("removes frozen words that SQLite has already replaced", () => {
    const persisted = createSegment("persisted", [
      { id: "word-old", startMs: 100 },
    ]);
    const live = createSegment("live", [
      { id: "word-replacement", startMs: 100 },
    ]);
    const request = createRequest(["word-replacement"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-replacement"]);
  });

  it("replaces a frozen word whose measured time no longer overlaps the live one", () => {
    // Fork 02.09.2026: der Nachbearbeitungsweg misst die Wortzeit jetzt, der
    // Live-Weg interpoliert sie weiter. Dasselbe "Bo" liegt deshalb
    // auseinander -- hier so weit, dass sich die beiden Spannen gar nicht mehr
    // beruehren. Die reine Ueberlappungsregel sah zwei Woerter und liess beide
    // stehen; auf dem Schirm stand "Bo Bo".
    const persisted = createSegment("persisted", [
      { id: "word-measured", startMs: 1_000, endMs: 1_120, text: " Bo" },
    ]);
    const live = createSegment("live", [
      { id: "word-live", startMs: 1_200, endMs: 1_600, text: " Bo" },
    ]);
    const request = createRequest(["word-measured"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-live"]);
  });

  it("keeps a genuinely repeated word when only one occurrence is replaced", () => {
    // Die Gegenrichtung, und die teurere: der Sprecher hat "ja" wirklich
    // zweimal gesagt. Ein einzelnes Live-"ja" darf nur EINES der beiden
    // ersetzen -- sonst verschwindet ein gesprochenes Wort still.
    const persisted = createSegment("persisted", [
      { id: "word-ja-1", startMs: 1_000, endMs: 1_200, text: " ja" },
      { id: "word-ja-2", startMs: 1_300, endMs: 1_500, text: " ja" },
    ]);
    const live = createSegment("live", [
      { id: "word-live-ja", startMs: 1_020, endMs: 1_220, text: " ja" },
    ]);
    const request = createRequest(["word-ja-1", "word-ja-2"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-live-ja", "word-ja-2"]);
  });

  it("pairs repeated words one to one instead of claiming the same one twice", () => {
    // Zweimal "ja" gesagt, zweimal live nachgereicht: jedes Live-Wort nimmt
    // GENAU EIN persistiertes. Ohne die mitlaufende Reihenfolge griffen beide
    // nach demselben, und das zweite persistierte "ja" blieb daneben stehen.
    const persisted = createSegment("persisted", [
      { id: "word-ja-1", startMs: 1_000, endMs: 1_200, text: " ja" },
      { id: "word-ja-2", startMs: 1_300, endMs: 1_500, text: " ja" },
    ]);
    const live = createSegment("live", [
      { id: "word-live-ja-1", startMs: 1_020, endMs: 1_220, text: " ja" },
      { id: "word-live-ja-2", startMs: 1_320, endMs: 1_520, text: " ja" },
    ]);
    const request = createRequest(["word-ja-1", "word-ja-2"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-live-ja-1", "word-live-ja-2"]);
  });

  it("does not replace a different word that happens to sit at the same time", () => {
    // Das Zeitfenster allein reicht nicht: ohne Textvergleich wuerde jedes
    // Wort in der Naehe als Ersatz durchgehen.
    const persisted = createSegment("persisted", [
      { id: "word-anders", startMs: 1_000, endMs: 1_120, text: " Sten" },
    ]);
    const live = createSegment("live", [
      { id: "word-live", startMs: 1_200, endMs: 1_600, text: " Bo" },
    ]);
    const request = createRequest(["word-anders"]);

    const merged = mergeRenderedAndLiveSegments([persisted], [live], request);

    expect(
      merged.flatMap((segment) => segment.words.map((word) => word.id)),
    ).toEqual(["word-anders", "word-live"]);
  });

  it("indexes pending replacements instead of scanning the full live tail", () => {
    const persisted = createSegment(
      "persisted",
      Array.from({ length: 1_000 }, (_, index) => ({
        id: `persisted-${index}`,
        startMs: index * 100,
      })),
    );
    const live = createSegment(
      "live",
      Array.from({ length: 1_000 }, (_, index) => ({
        id: `live-${index}`,
        startMs: 100_000 + index * 100,
      })),
    );
    const request = createRequest(
      [...persisted.words, ...live.words].flatMap((word) =>
        word.id ? [word.id] : [],
      ),
    );
    const hasSpy = vi.spyOn(Set.prototype, "has");

    try {
      mergeRenderedAndLiveSegments([persisted], [live], request);
      expect(hasSpy.mock.calls.length).toBeLessThan(10_000);
    } finally {
      hasSpy.mockRestore();
    }
  });
});

describe("applyRenderRequestIdentitiesToSegments", () => {
  it("applies a speaker assignment to current and future matching segments", () => {
    const current = createSegment("current", [
      { id: "word-current", startMs: 0 },
    ]);
    const future = createSegment("future", [
      { id: "word-future", startMs: 100 },
    ]);
    const other = createSegment("other", [{ id: "word-other", startMs: 200 }]);
    current.key.speaker_index = 1;
    future.key.speaker_index = 1;
    other.key.speaker_index = 2;
    const request = createRequest(
      ["word-current", "word-future", "word-other"],
      [
        {
          human_id: "human-1",
          scope: {
            kind: "channel_speaker",
            channel: "MixedCapture",
            speaker_index: 1,
          },
        },
      ],
    );

    const result = applyRenderRequestIdentitiesToSegments(
      [current, future, other],
      request,
    );

    expect(result.map((segment) => segment.key.speaker_human_id)).toEqual([
      "human-1",
      "human-1",
      null,
    ]);
  });

  it("splits word-scoped assignments and carries them into a trailing partial", () => {
    const segment = createSegment("live", [
      { id: "word-before", startMs: 0 },
      { id: "word-assigned", startMs: 100 },
      { startMs: 200 },
    ]);
    segment.key.speaker_index = 1;
    const request = createRequest(
      ["word-before", "word-assigned"],
      [
        {
          human_id: "speaker-human",
          scope: {
            kind: "channel_speaker",
            channel: "MixedCapture",
            speaker_index: 1,
          },
        },
        {
          human_id: "word-human",
          scope: { kind: "words", word_ids: ["word-assigned"] },
        },
      ],
    );

    const result = applyRenderRequestIdentitiesToSegments([segment], request);

    expect(result).toHaveLength(2);
    expect(
      result.map((current) => ({
        humanId: current.key.speaker_human_id,
        wordIds: current.words.map((word) => word.id),
      })),
    ).toEqual([
      { humanId: "speaker-human", wordIds: ["word-before"] },
      {
        humanId: "word-human",
        wordIds: ["word-assigned", undefined],
      },
    ]);
  });

  it("gives word assignments precedence over channel speaker assignments", () => {
    const segment = createSegment("live", [
      { id: "word-a", startMs: 0 },
      { id: "word-b", startMs: 100 },
    ]);
    segment.key.speaker_index = 1;
    const request = createRequest(
      ["word-a", "word-b"],
      [
        {
          human_id: "speaker-human",
          scope: {
            kind: "channel_speaker",
            channel: "MixedCapture",
            speaker_index: 1,
          },
        },
        {
          human_id: "word-human",
          scope: { kind: "words", word_ids: ["word-b"] },
        },
      ],
    );

    const result = applyRenderRequestIdentitiesToSegments([segment], request);

    expect(result.map((current) => current.key.speaker_human_id)).toEqual([
      "speaker-human",
      "word-human",
    ]);
  });

  it("keeps participant identity ahead of complete-channel assignments", () => {
    const segment = createSegment("live", [{ id: "word-a", startMs: 0 }]);
    segment.key = {
      channel: "DirectMic",
      speaker_index: null,
      speaker_human_id: "self",
    };
    const request = createRequest(
      ["word-a"],
      [
        {
          human_id: "other-human",
          scope: { kind: "channel", channel: "DirectMic" },
        },
      ],
    );
    request.self_human_id = "self";

    const result = applyRenderRequestIdentitiesToSegments([segment], request);

    expect(result[0]?.key.speaker_human_id).toBe("self");
  });
});

function createSegment(
  id: string,
  words: Array<{
    id?: string;
    startMs: number;
    endMs?: number;
    text?: string;
  }>,
): Segment {
  const wordText = (word: (typeof words)[number]) =>
    word.text ?? ` word-${word.id ?? "partial"}`;
  const wordEnd = (word: (typeof words)[number]) =>
    word.endMs ?? word.startMs + 100;

  return {
    id,
    key: {
      channel: "MixedCapture",
      speaker_index: null,
      speaker_human_id: null,
    },
    start_ms: words[0]?.startMs ?? 0,
    end_ms: words[words.length - 1] ? wordEnd(words[words.length - 1]!) : 0,
    text: words.map(wordText).join(""),
    words: words.map((word) => ({
      id: word.id,
      text: wordText(word),
      start_ms: word.startMs,
      end_ms: wordEnd(word),
      channel: "MixedCapture",
      is_final: Boolean(word.id),
    })),
  };
}

function createRequest(
  wordIds: string[],
  assignments: IdentityAssignment[] = [],
): RenderTranscriptRequest {
  return {
    humans: [],
    participant_human_ids: [],
    self_human_id: null,
    transcripts: [
      {
        assignments,
        started_at: null,
        words: wordIds.map((id, index) => ({
          id,
          text: id,
          start_ms: index * 100,
          end_ms: index * 100 + 100,
          channel: 2,
          speaker_index: null,
        })),
      },
    ],
  };
}
