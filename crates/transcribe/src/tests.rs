use super::apple::parse;
use super::{Segment, Track, merge};
use std::time::Duration;

fn segment(track: Track, start_ms: u64, end_ms: u64, text: &str) -> Segment {
    Segment {
        track,
        start: Duration::from_millis(start_ms),
        end: Duration::from_millis(end_ms),
        text: text.to_owned(),
    }
}

#[test]
fn parse_reads_one_segment_per_line() {
    let lines = "0\t1200\tHello there.\n1500\t2750\tIt's\tme \n\n";
    assert_eq!(
        parse(lines, Track::System).ok(),
        Some(vec![
            segment(Track::System, 0, 1200, "Hello there."),
            // Only the first two tabs split; the text keeps the rest.
            segment(Track::System, 1500, 2750, "It's\tme"),
        ])
    );
    assert_eq!(parse("", Track::Mic).ok(), Some(Vec::new()));
}

#[test]
fn parse_rejects_malformed_lines() {
    assert!(parse("12\tHello", Track::Mic).is_err());
    assert!(parse("a\t2\tHello", Track::Mic).is_err());
    assert!(parse("1\t2", Track::Mic).is_err());
}

#[test]
fn merge_orders_by_start_and_keeps_overlaps() {
    let mic = vec![
        segment(Track::Mic, 0, 2000, "Hi."),
        segment(Track::Mic, 5000, 6000, "Sure."),
    ];
    let system = vec![
        segment(Track::System, 1500, 4000, "Hello, can you hear me?"),
        segment(Track::System, 7000, 8000, "Great."),
    ];
    let texts: Vec<_> = merge(vec![mic, system])
        .into_iter()
        .map(|segment| segment.text)
        .collect();
    assert_eq!(texts, ["Hi.", "Hello, can you hear me?", "Sure.", "Great."]);
}

#[test]
fn merge_breaks_ties_by_end_then_mic_first() {
    let merged = merge(vec![
        vec![segment(Track::System, 1000, 2000, "b")],
        vec![
            segment(Track::Mic, 1000, 3000, "c"),
            segment(Track::Mic, 1000, 2000, "a"),
        ],
    ]);
    let order: Vec<_> = merged.iter().map(|segment| segment.text.as_str()).collect();
    assert_eq!(order, ["a", "b", "c"]);
}

#[test]
fn merge_takes_one_track_alone() {
    let system = vec![
        segment(Track::System, 3000, 4000, "later"),
        segment(Track::System, 0, 1000, "first"),
    ];
    let merged = merge(vec![system]);
    assert_eq!(merged[0].text, "first");
    assert_eq!(merged.len(), 2);
    assert!(merge(Vec::new()).is_empty());
}
