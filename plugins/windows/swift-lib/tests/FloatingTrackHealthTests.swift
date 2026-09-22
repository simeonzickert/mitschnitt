import XCTest

@testable import swift_lib

/// Parity tests for the macOS half of the Kanal-tot-Waechter.
///
/// These mirror `apps/desktop/src/meeting-float/track-health.test.ts` case for
/// case. If one side changes and the other does not, macOS and Windows start
/// disagreeing about whether a meeting is being recorded properly -- these tests
/// exist so that disagreement shows up here instead of in a lost conversation.
final class FloatingTrackHealthTests: XCTestCase {
  private let start: TimeInterval = 1_000

  private func levels(mic: Double, speaker: Double) -> FloatingTrackLevels {
    FloatingTrackLevels(mic: mic, speaker: speaker)
  }

  func testSaysNothingBeforeTheNeverHeardTimeoutElapses() {
    let watch = FloatingTrackWatch(startedAt: start)

    XCTAssertEqual(watch.silentTracks(at: start), [])
    XCTAssertEqual(watch.silentTracks(at: start + 11.9), [])
  }

  func testWarnsAboutBothTracksWhenNeitherWasEverHeard() {
    let watch = FloatingTrackWatch(startedAt: start)

    XCTAssertEqual(watch.silentTracks(at: start + 12), [.mic, .speaker])
    XCTAssertEqual(
      FloatingTrackHealth.describe(silentTracks: watch.silentTracks(at: start + 12)),
      "No audio on either track")
  }

  func testWarnsOnlyAboutTheTrackThatWasNeverHeard() {
    var watch = FloatingTrackWatch(startedAt: start)
    watch.observe(levels(mic: 0, speaker: 0.4), at: start + 1)

    XCTAssertEqual(watch.silentTracks(at: start + 12), [.mic])
    XCTAssertEqual(
      FloatingTrackHealth.describe(silentTracks: watch.silentTracks(at: start + 12)),
      "No microphone signal")
  }

  func testTreatsQuietSpeechAsSignal() {
    var watch = FloatingTrackWatch(startedAt: start)
    // Below the listener store's 0.05 "audible" bar, above our 0.02 floor.
    watch.observe(levels(mic: 0.02, speaker: 0.02), at: start + 1)

    XCTAssertEqual(watch.silentTracks(at: start + 12), [])
  }

  func testDoesNotCountAReadingBelowTheThresholdAsSignal() {
    var watch = FloatingTrackWatch(startedAt: start)
    watch.observe(levels(mic: 0.019, speaker: 0.019), at: start + 1)

    XCTAssertEqual(watch.silentTracks(at: start + 12), [.mic, .speaker])
  }

  func testGivesAHeardTrackThirtySecondsOfSilenceBeforeWarning() {
    var watch = FloatingTrackWatch(startedAt: start)
    watch.observe(levels(mic: 0.5, speaker: 0.5), at: start + 1)

    XCTAssertEqual(watch.silentTracks(at: start + 30.9), [])
    XCTAssertEqual(watch.silentTracks(at: start + 31), [.mic, .speaker])
  }

  func testWarnsAboutTheOtherSideWhenOnlyTheMicrophoneKeepsGoing() {
    var watch = FloatingTrackWatch(startedAt: start)
    watch.observe(levels(mic: 0.5, speaker: 0.5), at: start + 1)
    for step in stride(from: 2.0, through: 40.0, by: 1.0) {
      watch.observe(levels(mic: 0.5, speaker: 0), at: start + step)
    }

    XCTAssertEqual(watch.silentTracks(at: start + 40), [.speaker])
    XCTAssertEqual(
      FloatingTrackHealth.describe(silentTracks: watch.silentTracks(at: start + 40)),
      "No sound from the other side")
  }

  func testClearsTheWarningAsSoonAsSignalReturns() {
    var watch = FloatingTrackWatch(startedAt: start)
    watch.observe(levels(mic: 0.5, speaker: 0.5), at: start + 1)
    XCTAssertEqual(watch.silentTracks(at: start + 40), [.mic, .speaker])

    watch.observe(levels(mic: 0.5, speaker: 0.5), at: start + 41)
    XCTAssertEqual(watch.silentTracks(at: start + 41), [])
  }

  func testDescribesAHealthyRecordingAsNothingToSay() {
    XCTAssertNil(FloatingTrackHealth.describe(silentTracks: []))
  }
}
