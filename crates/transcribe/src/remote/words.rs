//! Segments made from timed words, for the APIs that time only words: ElevenLabs', xAI's and
//! Gemini's. A segment is what a reader takes in at once: it ends at a pause, at a sentence's
//! end once it has run a while, or before it gets too long.

use std::time::Duration;

/// A pause this long or longer starts a new segment.
const PAUSE: Duration = Duration::from_millis(800);
/// How long a segment runs before a sentence's end closes it.
const SENTENCE: Duration = Duration::from_secs(2);
/// The longest a segment of more than one word runs.
const LONGEST: Duration = Duration::from_secs(30);

/// A word, or a stretch of speech, timed within the piece of audio it was heard in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Timed {
    /// Where it starts in the piece.
    pub(super) start: Duration,
    /// Where it ends in the piece.
    pub(super) end: Duration,
    /// What was said.
    pub(super) text: String,
}

/// `words`, in the order they were said, as segments. A word starts a new segment when the
/// pause before it is [`PAUSE`] or longer, when the word before it ends a sentence and the
/// segment has run [`SENTENCE`], or when the segment would run past [`LONGEST`] with it. Words
/// are joined by single spaces, with none before punctuation or between Chinese or Japanese
/// words; empty ones are dropped.
pub(super) fn segments(words: Vec<Timed>) -> Vec<Timed> {
    let mut segments: Vec<Timed> = Vec::new();
    for word in words {
        let text = word.text.trim();
        if text.is_empty() {
            continue;
        }
        let end = word.end.max(word.start);
        match segments.last_mut() {
            Some(segment)
                if word.start.saturating_sub(segment.end) < PAUSE
                    && !(ends_sentence(&segment.text)
                        && segment.end.saturating_sub(segment.start) >= SENTENCE)
                    && end.saturating_sub(segment.start) <= LONGEST =>
            {
                if spaced(&segment.text, text) {
                    segment.text.push(' ');
                }
                segment.text.push_str(text);
                segment.end = segment.end.max(end);
            }
            _ => segments.push(Timed {
                start: word.start,
                end,
                text: text.to_owned(),
            }),
        }
    }
    segments
}

/// `words` as [`segments`], or, when an answer has none, its `text` as one segment spanning
/// the piece, `length` long.
pub(super) fn segments_or_text(words: Vec<Timed>, text: &str, length: Duration) -> Vec<Timed> {
    if words.is_empty() {
        return vec![Timed {
            start: Duration::ZERO,
            end: length,
            text: text.to_owned(),
        }];
    }
    segments(words)
}

/// Whether `text` ends a sentence, quotes and brackets after its last mark aside.
fn ends_sentence(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', '”', '’', ')', ']', '」', '』', '）'])
        .ends_with(['.', '?', '!', '…', '。', '？', '！'])
}

/// Whether a space goes between `before` and `word`: not before punctuation, and not between
/// Chinese or Japanese, which have no spaces between words.
fn spaced(before: &str, word: &str) -> bool {
    let punctuation = word.starts_with([
        ',', '.', '?', '!', ':', ';', '…', '、', '。', '，', '？', '！', '：', '；',
    ]);
    let unspaced = before.chars().next_back().is_some_and(unspaced_script)
        && word.chars().next().is_some_and(unspaced_script);
    !punctuation && !unspaced
}

/// Whether `c` is written without spaces between words: Chinese characters, kana, and the
/// punctuation and full-width forms that go with them.
fn unspaced_script(c: char) -> bool {
    matches!(
        c,
        '\u{3000}'..='\u{30FF}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{F900}'..='\u{FAFF}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

#[cfg(test)]
mod tests {
    use super::{Timed, segments};
    use std::time::Duration;

    fn timed(start_ms: u64, end_ms: u64, text: &str) -> Timed {
        Timed {
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            text: text.to_owned(),
        }
    }

    /// Words of 300 ms each, 100 ms apart, from `start_ms`.
    fn said(start_ms: u64, text: &str) -> Vec<Timed> {
        (0_u64..)
            .zip(text.split(' '))
            .map(|(at, word)| timed(start_ms + at * 400, start_ms + at * 400 + 300, word))
            .collect()
    }

    #[test]
    fn close_words_make_one_segment_and_a_pause_starts_another() {
        let mut words = said(0, "Hello there");
        words.extend(said(1_500, "Can you hear me?"));
        assert_eq!(
            segments(words),
            [
                timed(0, 700, "Hello there"),
                timed(1_500, 3_000, "Can you hear me?")
            ]
        );
        // Just under the pause, the words stay together.
        assert_eq!(
            segments(vec![timed(0, 500, "Hi"), timed(1_299, 1_600, "you")]),
            [timed(0, 1_600, "Hi you")]
        );
        assert_eq!(
            segments(vec![timed(0, 500, "Hi"), timed(1_300, 1_600, "you")]).len(),
            2
        );
    }

    #[test]
    fn a_sentence_s_end_closes_a_segment_only_once_it_has_run_two_seconds() {
        // "Yes." ends at 300 ms: too short to stand alone.
        assert_eq!(
            segments(said(0, "Yes. Go on.")),
            [timed(0, 1_100, "Yes. Go on.")]
        );
        let words = said(0, "This one runs on long enough. And this follows.");
        assert_eq!(
            segments(words),
            [
                timed(0, 2_300, "This one runs on long enough."),
                timed(2_400, 3_500, "And this follows.")
            ]
        );
        // Every mark that ends a sentence counts, quotes after it too.
        for end in [
            "done?", "done!", "done…", "done.\"", "done。", "done？", "done！",
        ] {
            let words = vec![timed(0, 2_000, end), timed(2_100, 2_400, "Next")];
            assert_eq!(segments(words).len(), 2, "{end}");
        }
        let words = vec![timed(0, 2_000, "done,"), timed(2_100, 2_400, "next")];
        assert_eq!(segments(words).len(), 1);
    }

    #[test]
    fn a_segment_never_runs_past_thirty_seconds() {
        // A word every 400 ms with no sentence's end, for 40 s.
        let words: Vec<Timed> = (0..100)
            .map(|at| timed(at * 400, at * 400 + 300, "word"))
            .collect();
        let made = segments(words);
        assert_eq!(made.len(), 2);
        assert_eq!(made[0].start, Duration::ZERO);
        assert!(made[0].end - made[0].start <= Duration::from_secs(30));
        assert_eq!(made[1].start, made[0].end + Duration::from_millis(100));
        assert_eq!(made[1].end, Duration::from_millis(39_900));
        // One word longer than that is still a segment.
        assert_eq!(
            segments(vec![timed(0, 31_000, "Hmmmm")]),
            [timed(0, 31_000, "Hmmmm")]
        );
    }

    #[test]
    fn words_join_with_single_spaces_but_none_before_punctuation() {
        let words = vec![
            timed(0, 100, " Well "),
            timed(100, 200, ","),
            timed(200, 300, "yes"),
            timed(300, 400, "?"),
            timed(400, 500, "   "),
            timed(500, 600, ""),
        ];
        assert_eq!(segments(words), [timed(0, 400, "Well, yes?")]);
        assert!(segments(Vec::new()).is_empty());
        assert!(segments(vec![timed(0, 100, " ")]).is_empty());
    }

    #[test]
    fn chinese_and_japanese_words_join_without_spaces() {
        let words = vec![
            timed(0, 300, "今日は"),
            timed(300, 600, "いい"),
            timed(600, 900, "天気"),
            timed(900, 1_000, "。"),
        ];
        assert_eq!(segments(words), [timed(0, 1_000, "今日はいい天気。")]);
        // Korean has spaces between words.
        let words = vec![timed(0, 300, "안녕"), timed(300, 600, "하세요")];
        assert_eq!(segments(words), [timed(0, 600, "안녕 하세요")]);
        let words = vec![timed(0, 300, "我用"), timed(300, 600, "Crumble")];
        assert_eq!(segments(words), [timed(0, 600, "我用 Crumble")]);
    }

    #[test]
    fn a_word_ending_before_it_starts_ends_where_it_starts() {
        assert_eq!(
            segments(vec![timed(2_000, 1_000, "odd")]),
            [timed(2_000, 2_000, "odd")]
        );
    }
}
