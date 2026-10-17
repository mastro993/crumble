//! The tabs of a Note's card and which one shows. GPUI-free.

/// One tab of the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteTab {
    Summary,
    Memo,
    Transcript,
}

impl NoteTab {
    pub(super) fn label(self) -> &'static str {
        match self {
            NoteTab::Summary => "Summary",
            NoteTab::Memo => "Memo",
            NoteTab::Transcript => "Transcript",
        }
    }
}

/// The card's tabs in order: the Summary first once the Note has one or one is being made, then
/// the Memo, then the Transcript once the Note has one or one is being made.
pub(super) fn tabs(summary: bool, transcript: bool) -> Vec<NoteTab> {
    let mut tabs = Vec::with_capacity(3);
    if summary {
        tabs.push(NoteTab::Summary);
    }
    tabs.push(NoteTab::Memo);
    if transcript {
        tabs.push(NoteTab::Transcript);
    }
    tabs
}

/// The tab that shows: the one the user picked while the Note has it, so it stays across Notes,
/// else the first.
pub(super) fn shown(picked: Option<NoteTab>, tabs: &[NoteTab]) -> NoteTab {
    picked
        .filter(|tab| tabs.contains(tab))
        .or_else(|| tabs.first().copied())
        .unwrap_or(NoteTab::Memo)
}

#[cfg(test)]
mod tests {
    use super::{NoteTab, shown, tabs};

    #[test]
    fn the_summary_comes_first_and_the_transcript_last_when_there_are_any() {
        assert_eq!(tabs(false, false), [NoteTab::Memo]);
        assert_eq!(tabs(false, true), [NoteTab::Memo, NoteTab::Transcript]);
        assert_eq!(
            tabs(true, true),
            [NoteTab::Summary, NoteTab::Memo, NoteTab::Transcript]
        );
    }

    #[test]
    fn the_picked_tab_shows_where_it_exists_else_the_first() {
        let all = tabs(true, true);
        assert_eq!(shown(None, &all), NoteTab::Summary);
        assert_eq!(shown(Some(NoteTab::Transcript), &all), NoteTab::Transcript);
        assert_eq!(shown(Some(NoteTab::Memo), &all), NoteTab::Memo);
        assert_eq!(
            shown(Some(NoteTab::Summary), &tabs(false, true)),
            NoteTab::Memo
        );
        assert_eq!(
            shown(Some(NoteTab::Transcript), &tabs(false, false)),
            NoteTab::Memo
        );
    }
}
