//! The tabs of a Note's card and which one shows. GPUI-free.

/// One tab of the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoteTab {
    Memo,
    Transcript,
}

impl NoteTab {
    pub(super) fn label(self) -> &'static str {
        match self {
            NoteTab::Memo => "Memo",
            NoteTab::Transcript => "Transcript",
        }
    }
}

/// The card's tabs in order: the Memo, then the Transcript once the Note has one or one is
/// being made.
pub(super) fn tabs(transcript: bool) -> Vec<NoteTab> {
    let mut tabs = vec![NoteTab::Memo];
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
    fn the_transcript_tab_comes_after_the_memo_only_when_there_is_one() {
        assert_eq!(tabs(false), [NoteTab::Memo]);
        assert_eq!(tabs(true), [NoteTab::Memo, NoteTab::Transcript]);
    }

    #[test]
    fn the_picked_tab_shows_where_it_exists_else_the_first() {
        let both = tabs(true);
        assert_eq!(shown(None, &both), NoteTab::Memo);
        assert_eq!(shown(Some(NoteTab::Transcript), &both), NoteTab::Transcript);
        assert_eq!(
            shown(Some(NoteTab::Transcript), &tabs(false)),
            NoteTab::Memo
        );
        assert_eq!(shown(Some(NoteTab::Memo), &both), NoteTab::Memo);
    }
}
