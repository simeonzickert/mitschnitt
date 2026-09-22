import Combine
import Foundation

final class FloatingBarViewModel: ObservableObject {
  @Published var amplitude: Double = 0
  @Published var status: FloatingBarStatus = .recording
  @Published var colorScheme: FloatingBarColorScheme = .dark
  @Published var isExpanded: Bool = false
  @Published var liveCaptionToggleVisible: Bool = false
  @Published var title: String = "Live transcript"
  @Published var transcriptBubbles: [FloatingTranscriptBubblePayload] = []

  /// Per-track levels for the Kanal-tot-Waechter. Only meaningful while
  /// `hasTrackLevels` is true.
  @Published var trackLevels = FloatingTrackLevels(mic: 0, speaker: 0)

  /// Whether the per-track channel has ever delivered. Until it does we know
  /// only the collapsed `amplitude`, which cannot tell a dead track from a
  /// quiet one -- so the bar keeps its single-waveform look and stays silent
  /// rather than guessing. A watch that guesses is worse than no watch.
  @Published var hasTrackLevels: Bool = false

  /// Tracks currently judged dead. Empty means all is well.
  @Published var silentTracks: [FloatingTrack] = []
}
