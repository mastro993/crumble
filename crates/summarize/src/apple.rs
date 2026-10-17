//! Apple's on-device model, through the Swift bridge. Its context holds about 4,000 tokens, so a
//! long Transcript is summarized in parts: notes on each part, then a Summary of the notes.

use anyhow::bail;

use crate::chunk::chunks;
use crate::{Summarizer, ffi, prompt};
use crumble_remote::Provider;

/// The tokens left for each answer: notes on a part, or the Summary.
const RESPONSE_TOKENS: usize = 1000;
/// Tokens kept free for the prompt's label, and for counts of items apart that differ from
/// their count together.
const MARGIN: usize = 100;
/// Rounds of notes on notes before giving up. Each round's notes fit in a fraction of the
/// budget, so two rounds cover hours of speech.
const MAX_ROUNDS: usize = 4;

/// Apple's on-device Foundation Models: nothing leaves the Mac.
pub struct Apple;

impl Summarizer for Apple {
    fn provider(&self) -> Provider {
        Provider::Apple
    }

    fn model(&self) -> &str {
        "Foundation Models"
    }

    fn summarize(&self, transcript: &str, progress: &mut dyn FnMut(f32)) -> anyhow::Result<String> {
        let longest = [true, false]
            .into_iter()
            .flat_map(|from_notes| {
                [
                    prompt::summary_instructions(from_notes),
                    prompt::part_instructions(from_notes),
                ]
            })
            .collect::<Vec<_>>();
        let longest = longest.iter().map(String::as_str).collect::<Vec<_>>();
        let instructions = ffi::token_counts(&longest)?.into_iter().max().unwrap_or(0);
        let Some(budget) = ffi::context_size().checked_sub(instructions + RESPONSE_TOKENS + MARGIN)
        else {
            bail!("Apple's on-device model has too small a context to summarize.");
        };
        in_parts(
            transcript.lines().filter(|line| !line.trim().is_empty()),
            budget,
            ffi::token_counts,
            |instructions, text| ffi::respond(instructions, text, RESPONSE_TOKENS),
            progress,
        )
    }
}

/// Summarizes `lines` with a model that reads `budget` tokens of text at once: in one go when
/// they fit, else as notes on each part that fits, then notes on those notes, until they fit.
/// `count` gives each text's tokens, and `respond` answers instructions and a prompt.
fn in_parts<'a>(
    lines: impl Iterator<Item = &'a str>,
    budget: usize,
    mut count: impl FnMut(&[&str]) -> anyhow::Result<Vec<usize>>,
    mut respond: impl FnMut(&str, &str) -> anyhow::Result<String>,
    progress: &mut dyn FnMut(f32),
) -> anyhow::Result<String> {
    let mut items: Vec<String> = lines.map(str::to_owned).collect();
    let (mut from_notes, mut answered, mut reported) = (false, 0_usize, 0.0_f32);
    for _ in 0..MAX_ROUNDS {
        let texts: Vec<&str> = items.iter().map(String::as_str).collect();
        // Each item also takes the line break that joins it to the next.
        let costs: Vec<usize> = count(&texts)?.into_iter().map(|cost| cost + 1).collect();
        let parts = chunks(&costs, budget);
        if parts.len() <= 1 {
            let summary = respond(
                &prompt::summary_instructions(from_notes),
                &prompt::prompt(&texts.join("\n"), from_notes),
            )?;
            progress(1.0);
            return Ok(summary);
        }
        let instructions = prompt::part_instructions(from_notes);
        let mut notes = Vec::with_capacity(parts.len());
        for (index, part) in parts.iter().enumerate() {
            notes.push(respond(
                &instructions,
                &prompt::prompt(&texts[part.clone()].join("\n"), from_notes),
            )?);
            answered += 1;
            // ponytail: later rounds are unknown, so each counts as one more answer; the bar
            // only moves forward.
            #[allow(clippy::cast_precision_loss, reason = "a few hundred answers at most")]
            let fraction = answered as f32 / (answered + parts.len() - index) as f32;
            reported = reported.max(fraction);
            progress(reported);
        }
        items = notes;
        from_notes = true;
    }
    bail!("The Transcript was too long for Apple's on-device model.")
}

#[cfg(test)]
mod tests {
    use super::in_parts;

    /// One token per byte, plus one per line break, and an answer that is its prompt's first line, cut to 6 bytes.
    fn run(lines: &[&str], budget: usize) -> (anyhow::Result<String>, Vec<String>, Vec<f32>) {
        let mut prompts = Vec::new();
        let mut fractions = Vec::new();
        let result = in_parts(
            lines.iter().copied(),
            budget,
            |texts| Ok(texts.iter().map(|text| text.len()).collect()),
            |_, prompt| {
                prompts.push(prompt.to_owned());
                let text = prompt.split_once("\n\n").map_or("", |(_, text)| text);
                Ok(text.lines().next().unwrap_or("").chars().take(6).collect())
            },
            &mut |fraction| fractions.push(fraction),
        );
        (result, prompts, fractions)
    }

    #[test]
    fn a_short_transcript_is_summarized_at_once() {
        let (summary, prompts, fractions) = run(&["You: Hi.", "Others: Hello."], 100);
        assert_eq!(summary.ok().as_deref(), Some("You: H"));
        assert_eq!(prompts, ["Transcript:\n\nYou: Hi.\nOthers: Hello."]);
        assert_eq!(fractions, [1.0]);
    }

    #[test]
    fn a_long_transcript_is_summarized_from_notes_on_its_parts() {
        let lines = ["You: aaaaaaaaaa", "Others: bbbbbbbbb", "You: cccccccccc"];
        let (summary, prompts, fractions) = run(&lines, 21);
        assert!(summary.is_ok());
        assert_eq!(
            prompts,
            [
                "Transcript:\n\nYou: aaaaaaaaaa",
                "Transcript:\n\nOthers: bbbbbbbbb",
                "Transcript:\n\nYou: cccccccccc",
                "Notes:\n\nYou: a\nOthers\nYou: c",
            ]
        );
        assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(fractions.last(), Some(&1.0));
    }

    #[test]
    fn notes_that_never_fit_give_up() {
        // Every line is over budget, and so is every note on it.
        let (summary, prompts, _) = run(&["aaaaaaaaaaaa", "bbbbbbbbbbbb"], 4);
        assert!(summary.is_err());
        assert_eq!(prompts.len(), 2 * super::MAX_ROUNDS);
    }
}
