//! The time beside the Now line: whether the pointer is on the line, and how far the time has
//! slid and faded in as it enters, or out as it leaves.

use std::time::{Duration, Instant};

/// How long the time takes to enter or leave.
const SLIDE: Duration = Duration::from_millis(180);

/// Whether the pointer is on the Now line, and the time's slide toward showing that.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct NowHover {
    over: bool,
    /// How far the time had entered when the pointer last came or went, and when.
    from: f32,
    since: Option<Instant>,
}

impl NowHover {
    /// The pointer came onto the line or left it at `at`; a slide under way turns around where
    /// it is.
    pub(super) fn set(&mut self, over: bool, at: Instant) {
        if over != self.over {
            self.from = self.shown(at);
            self.over = over;
            self.since = Some(at);
        }
    }

    /// Whether the pointer is on the line.
    pub(super) fn over(self) -> bool {
        self.over
    }

    /// How far the time has entered at `at`, from 0, gone below its place, to 1, in place.
    pub(super) fn shown(self, at: Instant) -> f32 {
        let target = f32::from(u8::from(self.over));
        let Some(since) = self.since else {
            return target;
        };
        let t = (at.saturating_duration_since(since).as_secs_f32() / SLIDE.as_secs_f32()).min(1.0);
        // Ease out: quick to start, settling into place.
        self.from + (target - self.from) * (1.0 - (1.0 - t).powi(3))
    }

    /// Whether the time is still sliding at `at`.
    pub(super) fn sliding(self, at: Instant) -> bool {
        self.since
            .is_some_and(|since| at.saturating_duration_since(since) < SLIDE)
    }
}

#[cfg(test)]
mod tests {
    use super::{NowHover, SLIDE};
    use std::time::{Duration, Instant};

    #[test]
    fn the_time_slides_in_and_out_and_turns_around_where_it_is() {
        let start = Instant::now();
        let mut hover = NowHover::default();
        assert_eq!(hover.shown(start), 0.0);
        assert!(!hover.sliding(start));

        hover.set(true, start);
        assert_eq!(hover.shown(start), 0.0);
        assert!(hover.sliding(start));
        let halfway = start + SLIDE / 2;
        let entered = hover.shown(halfway);
        assert!(entered > 0.5 && entered < 1.0);
        assert_eq!(hover.shown(start + SLIDE), 1.0);
        assert!(!hover.sliding(start + SLIDE));

        // Leaving halfway in starts the slide out from there.
        hover.set(false, halfway);
        assert_eq!(hover.shown(halfway), entered);
        assert_eq!(hover.shown(halfway + SLIDE), 0.0);

        // Setting the same state again does not restart the slide.
        hover.set(false, halfway + Duration::from_millis(50));
        assert_eq!(hover.shown(halfway + SLIDE), 0.0);
    }
}
