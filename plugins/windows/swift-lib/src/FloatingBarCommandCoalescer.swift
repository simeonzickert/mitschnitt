import Foundation

final class FloatingBarCommandCoalescer {
  enum Action {
    case show
    case hide
    case update(FloatingBarStatePayload)
    case amplitude(Double)
    case trackLevels(FloatingTrackLevels)
    case resetTrackWatch
  }

  typealias Scheduler = (@escaping () -> Void) -> Void

  private struct SequencedAction {
    let sequence: UInt64
    let action: Action
  }

  private let lock = NSLock()
  private let scheduler: Scheduler
  private let apply: (Action) -> Void
  private var nextSequence: UInt64 = 0
  private var pendingVisibility: SequencedAction?
  private var pendingUpdate: SequencedAction?
  private var pendingTrackWatchReset: SequencedAction?
  private var pendingTrackLevels: SequencedAction?
  private var drainScheduled = false

  init(
    scheduler: @escaping Scheduler = { work in
      DispatchQueue.main.async(execute: work)
    },
    apply: @escaping (Action) -> Void
  ) {
    self.scheduler = scheduler
    self.apply = apply
  }

  func enqueueShow() {
    enqueue { sequence in
      pendingVisibility = SequencedAction(sequence: sequence, action: .show)
    }
  }

  func enqueueHide() {
    enqueue { sequence in
      pendingVisibility = SequencedAction(sequence: sequence, action: .hide)
    }
  }

  func enqueueUpdate(_ state: FloatingBarStatePayload) {
    enqueue { sequence in
      let previousState: FloatingBarStatePayload?
      if let pendingUpdate, case .update(let state) = pendingUpdate.action {
        previousState = state
      } else {
        previousState = nil
      }
      let state = state.preservingTranscriptBubbles(from: previousState)
      pendingUpdate = SequencedAction(sequence: sequence, action: .update(state))
    }
  }

  func enqueueAmplitude(_ amplitude: Double) {
    enqueue { sequence in
      let action: Action
      if let pendingUpdate, case .update(let state) = pendingUpdate.action {
        action = .update(state.replacingAmplitude(with: amplitude))
      } else {
        action = .amplitude(amplitude)
      }
      pendingUpdate = SequencedAction(sequence: sequence, action: action)
    }
  }

  /// Track levels are coalesced by PEAK, not by replacement.
  ///
  /// Every other command here is a "latest wins" state push, but the dead-track
  /// watch asks a different question: was there ANY signal since the last look?
  /// Overwriting a burst with the reading that happened to arrive last would
  /// throw away a spike that proves a track is alive, and the watch would then
  /// declare a living track dead. Taking the elementwise maximum keeps that
  /// answer exact while staying O(1).
  func enqueueTrackLevels(_ levels: FloatingTrackLevels) {
    enqueue { sequence in
      let merged: FloatingTrackLevels
      if let pendingTrackLevels, case .trackLevels(let pending) = pendingTrackLevels.action {
        merged = FloatingTrackLevels(
          mic: max(pending.mic, levels.mic),
          speaker: max(pending.speaker, levels.speaker))
      } else {
        merged = levels
      }
      pendingTrackLevels = SequencedAction(sequence: sequence, action: .trackLevels(merged))
    }
  }

  func enqueueResetTrackWatch() {
    enqueue { sequence in
      // Levels observed before the reset belong to the recording that just
      // ended; carrying them across would seed the new watch with old signal.
      pendingTrackLevels = nil
      pendingTrackWatchReset = SequencedAction(sequence: sequence, action: .resetTrackWatch)
    }
  }

  private func enqueue(_ merge: (UInt64) -> Void) {
    lock.lock()
    nextSequence &+= 1
    merge(nextSequence)
    let shouldSchedule = !drainScheduled
    drainScheduled = true
    lock.unlock()

    if shouldSchedule {
      scheduleDrain()
    }
  }

  private func scheduleDrain() {
    scheduler { [weak self] in
      self?.drain()
    }
  }

  private func drain() {
    lock.lock()
    let actions = [pendingVisibility, pendingUpdate, pendingTrackWatchReset, pendingTrackLevels]
      .compactMap { $0 }
      .sorted { $0.sequence < $1.sequence }
    pendingVisibility = nil
    pendingUpdate = nil
    pendingTrackWatchReset = nil
    pendingTrackLevels = nil
    if actions.isEmpty {
      drainScheduled = false
    }
    lock.unlock()

    guard !actions.isEmpty else { return }

    for action in actions {
      apply(action.action)
    }

    lock.lock()
    let hasPendingActions =
      pendingVisibility != nil || pendingUpdate != nil
      || pendingTrackWatchReset != nil || pendingTrackLevels != nil
    if !hasPendingActions {
      drainScheduled = false
    }
    lock.unlock()

    if hasPendingActions {
      scheduleDrain()
    }
  }
}

extension FloatingBarStatePayload {
  fileprivate func preservingTranscriptBubbles(
    from previousState: FloatingBarStatePayload?
  ) -> FloatingBarStatePayload {
    guard transcriptBubbles == nil,
      let transcriptBubbles = previousState?.transcriptBubbles
    else {
      return self
    }

    return FloatingBarStatePayload(
      amplitude: amplitude,
      title: title,
      status: status,
      colorScheme: colorScheme,
      opacity: opacity,
      liveCaptionOpacity: liveCaptionOpacity,
      liveCaptionWidth: liveCaptionWidth,
      liveCaptionLineCount: liveCaptionLineCount,
      liveCaptionPosition: liveCaptionPosition,
      liveCaptionMinimized: liveCaptionMinimized,
      liveCaptionToggleVisible: liveCaptionToggleVisible,
      transcriptBubbles: transcriptBubbles)
  }

  fileprivate func replacingAmplitude(with amplitude: Double) -> FloatingBarStatePayload {
    FloatingBarStatePayload(
      amplitude: amplitude,
      title: title,
      status: status,
      colorScheme: colorScheme,
      opacity: opacity,
      liveCaptionOpacity: liveCaptionOpacity,
      liveCaptionWidth: liveCaptionWidth,
      liveCaptionLineCount: liveCaptionLineCount,
      liveCaptionPosition: liveCaptionPosition,
      liveCaptionMinimized: liveCaptionMinimized,
      liveCaptionToggleVisible: liveCaptionToggleVisible,
      transcriptBubbles: transcriptBubbles)
  }
}
