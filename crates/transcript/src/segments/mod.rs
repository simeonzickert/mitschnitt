mod anzeige;
mod collect;
mod model;
mod normalize;
mod speakers;

#[cfg(test)]
mod tests;

use crate::types::{FinalizedWord, IdentityAssignment, PartialWord};
use crate::types::{Segment, SegmentBuilderOptions};

pub use self::anzeige::{MAX_BRUECKE_MS, derselbe_sprecher, verschmelze_anzeige_nachbarn};
use self::collect::{collect_segments, finalize_segments, propagate_identity};
use self::normalize::normalize_words;
use self::speakers::{create_speaker_state, resolve_identities};

pub fn build_segments(
    final_words: &[FinalizedWord],
    partial_words: &[PartialWord],
    assignments: &[IdentityAssignment],
    options: Option<&SegmentBuilderOptions>,
) -> Vec<Segment> {
    if final_words.is_empty() && partial_words.is_empty() {
        return Vec::new();
    }

    let words = normalize_words(final_words, partial_words);
    let mut speaker_state = create_speaker_state(assignments, &words, options);

    let frames = resolve_identities(&words, &mut speaker_state);
    let mut proto_segments = collect_segments(frames, options);
    propagate_identity(&mut proto_segments, &speaker_state, options);

    finalize_segments(proto_segments)
}
