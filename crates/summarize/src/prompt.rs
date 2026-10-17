//! What every Model is told: the instructions for a Summary, for notes on one part of a long
//! Transcript, and the prompts that carry the text.

/// How the Transcript reads, which every instruction starts from.
const TRANSCRIPT: &str = "The transcript has one line per stretch of speech. Lines starting \
\"You:\" are what the person who recorded the meeting said; lines starting \"Others:\" are \
everyone else on the call, who may be several people. It comes from speech recognition, so it \
can have misheard words: read through them.";

/// What no answer may do.
const FAITHFUL: &str = "Use only what the text says. Do not invent names, numbers, dates, \
decisions or tasks, and do not turn a suggestion into a decision. Reply with the result alone, \
with no preamble.";

/// The instructions for a Summary of a Transcript, or of the notes on its parts when
/// `from_notes`.
pub(crate) fn summary_instructions(from_notes: bool) -> String {
    let source = if from_notes {
        "You are given notes taken on consecutive parts of one meeting, in order. They were \
         written from its transcript. "
    } else {
        ""
    };
    format!(
        "You summarize meetings from their transcript. {source}{TRANSCRIPT}\n\n\
         Write the summary in the language the meeting was held in, as Markdown:\n\
         - First, a short paragraph saying what the meeting was about and how it ended.\n\
         - Then a \"## Key points\" section of bullet points.\n\
         - Then a \"## Decisions\" section of bullet points.\n\
         - Then an \"## Action items\" section of bullet points, each naming who will do it \
         when the meeting said so.\n\
         Write the section titles in that language too, and \"None.\" under a section with \
         nothing in it.\n\n\
         {FAITHFUL}"
    )
}

/// The instructions for notes on one part of a Transcript, or of earlier notes when
/// `from_notes`, to summarize together later.
pub(crate) fn part_instructions(from_notes: bool) -> String {
    let source = if from_notes {
        "You are given notes on consecutive parts of one meeting. Merge them into one shorter \
         set of notes."
    } else {
        "You are given one part of a longer meeting's transcript. Take notes on it."
    };
    format!(
        "{source} {TRANSCRIPT}\n\n\
         Write short bullet points, in the language the meeting was held in, of what was said, \
         what was decided, and who will do what. Keep names, numbers and dates exactly.\n\n\
         {FAITHFUL}"
    )
}

/// The prompt carrying `text`: a Transcript, or notes when `from_notes`.
pub(crate) fn prompt(text: &str, from_notes: bool) -> String {
    let label = if from_notes { "Notes" } else { "Transcript" };
    format!("{label}:\n\n{text}")
}

#[cfg(test)]
mod tests {
    use super::{part_instructions, prompt, summary_instructions};

    #[test]
    fn instructions_ask_for_the_sections_in_the_meetings_language() {
        let summary = summary_instructions(false);
        for asked in [
            "## Key points",
            "## Decisions",
            "## Action items",
            "language the meeting was held in",
            "\"You:\"",
            "\"Others:\"",
            "Do not invent",
        ] {
            assert!(summary.contains(asked), "{asked}");
        }
        assert!(summary_instructions(true).contains("notes taken on consecutive parts"));
        assert!(part_instructions(false).contains("Do not invent"));
        assert!(part_instructions(true).contains("Merge them"));
        assert_eq!(prompt("You: Hi.", false), "Transcript:\n\nYou: Hi.");
        assert_eq!(prompt("- Hi", true), "Notes:\n\n- Hi");
    }
}
