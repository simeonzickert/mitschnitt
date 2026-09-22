import Foundation

/// Kanal-tot-Waechter -- native macOS half of the dead-track watch.
///
/// A recording writes two separate tracks (`audio_mic.wav` / `audio_spk.wav`).
/// When one of them dies mid-meeting nothing in the UI says so, because the bar
/// only ever saw ONE amplitude: the two tracks collapsed into a single scalar,
/// so the living track keeps the number up and hides the loss until after the
/// meeting, when the conversation is gone.
///
/// This is the pure decision layer. It takes per-track levels over time and
/// answers one question: which track has been practically silent long enough
/// that a human should look at it now?
///
/// It is a deliberate 1:1 port of
/// `apps/desktop/src/meeting-float/track-health.ts`, which drives the same
/// watch inside the React overlay used on Windows and Linux. Both surfaces must
/// reach the same verdict from the same numbers, so the thresholds and the
/// message wording below are copied, not re-derived. Change one side and the
/// two platforms start disagreeing about whether a meeting is being recorded
/// properly -- so change both.
enum FloatingTrack: String, CaseIterable, Equatable {
  case mic
  case speaker
}

struct FloatingTrackLevels: Equatable {
  /// Microphone track, normalized 0..1.
  var mic: Double
  /// System audio track (the other party), normalized 0..1.
  var speaker: Double

  func level(for track: FloatingTrack) -> Double {
    switch track {
    case .mic:
      return mic
    case .speaker:
      return speaker
    }
  }
}

enum FloatingTrackHealth {
  /// Anything at or above this counts as "the track is alive".
  ///
  /// The listener store already treats 0.05 as "audible" for its transcription
  /// stall watchdog. We sit deliberately below that: quiet speech, a distant
  /// speaker or a compressed conference stream must still count as signal. What
  /// we are looking for is a track that is flat, not a track that is quiet.
  static let signalThreshold: Double = 0.02

  /// A track that HAS carried sound may fall silent for this long before we
  /// warn. Natural pauses, someone thinking, a screen-share monologue on one
  /// side -- none of that should raise an alarm.
  static let silenceTimeout: TimeInterval = 30

  /// A track that has NEVER carried sound since the recording started is the
  /// clearest case there is (wrong input device, permission missing, cable
  /// out), so it may warn sooner.
  static let neverHeardTimeout: TimeInterval = 12

  /// Plain-language message naming WHICH track is silent, or nil when there is
  /// nothing to say. Kept in the overlay's own (English) register -- the
  /// floating bar does not go through the app's i18n layer. Wording is shared
  /// verbatim with `describeSilentTracks` in track-health.ts.
  static func describe(silentTracks tracks: [FloatingTrack]) -> String? {
    let mic = tracks.contains(.mic)
    let speaker = tracks.contains(.speaker)

    if mic && speaker {
      return "No audio on either track"
    }

    if mic {
      return "No microphone signal"
    }

    if speaker {
      return "No sound from the other side"
    }

    return nil
  }
}

/// Rolling record of when each track was last heard.
///
/// All timestamps are caller-supplied so the watch stays testable and so the
/// production path can feed it a monotonic clock (`systemUptime`) rather than
/// wall time, which a clock change could move backwards mid-meeting.
struct FloatingTrackWatch: Equatable {
  /// When the current recording started being watched.
  let startedAt: TimeInterval
  /// Last moment each track was at or above the signal threshold.
  private(set) var lastSignalAt: [FloatingTrack: TimeInterval] = [:]

  init(startedAt: TimeInterval) {
    self.startedAt = startedAt
  }

  /// Fold one level reading into the watch.
  mutating func observe(_ levels: FloatingTrackLevels, at now: TimeInterval) {
    for track in FloatingTrack.allCases
    where levels.level(for: track) >= FloatingTrackHealth.signalThreshold {
      lastSignalAt[track] = now
    }
  }

  func isSilent(_ track: FloatingTrack, at now: TimeInterval) -> Bool {
    guard let lastSignalAt = lastSignalAt[track] else {
      return now - startedAt >= FloatingTrackHealth.neverHeardTimeout
    }

    return now - lastSignalAt >= FloatingTrackHealth.silenceTimeout
  }

  /// Which tracks are currently judged dead. Empty means all is well.
  func silentTracks(at now: TimeInterval) -> [FloatingTrack] {
    FloatingTrack.allCases.filter { isSilent($0, at: now) }
  }
}
