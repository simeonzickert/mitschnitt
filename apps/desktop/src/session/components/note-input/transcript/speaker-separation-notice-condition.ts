/**
 * Hinweis an alten Cloud-Transkripten ohne Kanaltrennung (ZICK-330 Nachzug).
 *
 * Bis zum Kanal-Weg ging eine Stereo-Aufnahme als EINE Datei an den
 * Cloud-Anbieter, und alle Woerter landeten auf Kanal 0. Ein Neu-Transkribieren
 * trennt heute Mikrofon und Systemton. Ob es das fuer DIESE Sitzung wirklich
 * taete, entscheidet Rust (`cloud_channel_split_available`: Anbieter auf der
 * Positivliste, zwei sprechende Kanaele, nicht dieselbe Quelle). Hier steht
 * nur der billige Vorfilter, damit Rust die Aufnahme nicht bei jeder Sitzung
 * dekodieren muss.
 */
export type TranscriptChannelSummary = {
  provider: string | null;
  model: string | null;
  wordCount: number;
  /** Ueber ALLE Transkript-Zeilen der Sitzung, wie die Trennungsregel. */
  sessionChannelCount: number;
  sessionProviderSpeakerCount: number;
  sessionAssignedHumanCount: number;
};

export type UnsplitTranscriptCandidate = {
  provider: string;
  model: string | null;
};

/**
 * Kandidat, wenn jede Transkript-Zeile Woerter hat und das Transkript nach
 * DERSELBEN Regel wie `transcriptHasSpeakerSeparation`
 * (`stt/speaker-separation.ts`, Rust: `crates/transcript/src/separation.rs`)
 * ungetrennt ist: hoechstens ein Kanal, hoechstens ein Anbieter-Sprecher,
 * hoechstens ein automatisch zugeordneter Mensch (Forge m8). Woerter ohne
 * Kanal zaehlen als ungetrennt (Forge M5). Handzuordnungen trennen nicht.
 * Anbieter und Modell der letzten Zeile, denn mit ihnen liefe das
 * Neu-Transkribieren.
 */
export function unsplitTranscriptCandidate(
  transcripts: TranscriptChannelSummary[],
): UnsplitTranscriptCandidate | null {
  if (transcripts.length === 0) {
    return null;
  }
  if (transcripts.some((transcript) => transcript.wordCount === 0)) {
    return null;
  }
  const session = transcripts[0]!;
  if (
    session.sessionChannelCount > 1 ||
    session.sessionProviderSpeakerCount > 1 ||
    session.sessionAssignedHumanCount > 1
  ) {
    return null;
  }
  const last = transcripts[transcripts.length - 1]!;
  if (!last.provider) {
    return null;
  }
  return { provider: last.provider, model: last.model };
}

const DISMISSED_PREFIX = "mitschnitt.speaker-separation-notice.dismissed.";

export function isSpeakerSeparationNoticeDismissed(sessionId: string): boolean {
  try {
    return window.localStorage.getItem(DISMISSED_PREFIX + sessionId) === "1";
  } catch {
    return false;
  }
}

export function dismissSpeakerSeparationNotice(sessionId: string): void {
  try {
    window.localStorage.setItem(DISMISSED_PREFIX + sessionId, "1");
  } catch {
    // Kein Speicher (privates Fenster, gesperrt): der Hinweis ist dann nur fuer
    // diese Ansicht weg, das reicht.
  }
}
