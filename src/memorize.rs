//! Memorization practice: progressive cloze deletion and recall scoring.

use crate::diff::{diff_tokens, token_key, DiffOp};
use crate::hashing::{fnv1a, splitmix64};
use crate::verses::Verse;

/// The hardest level: every word blanked.
pub const MAX_LEVEL: u8 = 5;

/// A stable per-verse salt, so a verse hides the same words every time
/// (`seed` reshuffles them).
pub fn verse_salt(verse: &Verse, seed: u64) -> u64 {
    let id = format!("{}.{}.{}", verse.book, verse.chapter, verse.verse);
    fnv1a(id.as_bytes()) ^ splitmix64(seed)
}

/// Hide part of a verse for practice. Levels 1-3 blank a quarter, half, and
/// three quarters of the words; 4 keeps only each word's first letter; 5 blanks
/// every word; 0 is the full text. Each level hides a superset of the level
/// below it, so the words you just practiced stay hidden as you level up.
pub fn cloze(text: &str, level: u8, salt: u64) -> String {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let words: Vec<usize> = (0..tokens.len()).filter(|&i| is_word(tokens[i])).collect();

    let mut hidden = vec![false; tokens.len()];
    match level {
        0 => {}
        1..=3 => {
            // A fixed per-verse shuffle of the words; each level hides a longer
            // prefix of it (at least one word).
            let mut order = words.clone();
            order.sort_by_key(|&i| splitmix64(salt ^ i as u64));
            let count = (words.len() * level as usize).div_ceil(4);
            for &i in &order[..count] {
                hidden[i] = true;
            }
        }
        _ => {
            for &i in &words {
                hidden[i] = true;
            }
        }
    }

    tokens
        .iter()
        .zip(&hidden)
        .map(|(token, &hide)| match (hide, level) {
            (false, _) => token.to_string(),
            (true, 4) => initial(token),
            (true, _) => blank(token),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A token that holds a word, as opposed to bare punctuation like an em dash.
fn is_word(token: &str) -> bool {
    token.chars().any(char::is_alphanumeric)
}

/// `world,` -> `_____,`: letters become blanks, punctuation stays as a hint.
fn blank(token: &str) -> String {
    token
        .chars()
        .map(|c| if c.is_alphanumeric() { '_' } else { c })
        .collect()
}

/// `world,` -> `w,` and `(and` -> `(a`: the first letter, keeping the leading
/// and trailing punctuation.
fn initial(token: &str) -> String {
    let Some(first) = token.find(char::is_alphanumeric) else {
        return token.to_string();
    };
    let first_char = token[first..].chars().next().unwrap_or_default();
    let tail = token
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| &token[i + c.len_utf8()..])
        .unwrap_or("");
    format!("{}{}{}", &token[..first], first_char, tail)
}

/// Number of words in a verse (bare punctuation does not count).
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().filter(|t| is_word(t)).count()
}

/// How a typed recitation compares with a verse, word by word.
pub struct Recall<'a> {
    /// The verse's tokens, each with whether it was recalled. Bare
    /// punctuation is never scored and is always `true`.
    pub tokens: Vec<(&'a str, bool)>,
    pub correct: usize,
    pub total: usize,
    /// Typed words that are not in the verse.
    pub extra: usize,
}

impl Recall<'_> {
    pub fn percent(&self) -> usize {
        // An empty verse has nothing to miss.
        (self.correct * 100).checked_div(self.total).unwrap_or(100)
    }

    pub fn is_perfect(&self) -> bool {
        self.correct == self.total && self.extra == 0
    }
}

/// Score a recitation against a verse with the word-level diff: case and
/// punctuation are ignored, word order matters.
pub fn score_recall<'a>(expected: &'a str, typed: &str) -> Recall<'a> {
    let all: Vec<&'a str> = expected.split_whitespace().collect();
    let word_positions: Vec<usize> = (0..all.len())
        .filter(|&i| !token_key(all[i]).is_empty())
        .collect();
    let expected_words: Vec<&str> = word_positions.iter().map(|&i| all[i]).collect();
    let typed_words: Vec<&str> = typed
        .split_whitespace()
        .filter(|t| !token_key(t).is_empty())
        .collect();

    let mut tokens: Vec<(&'a str, bool)> = all.iter().map(|&t| (t, true)).collect();
    for &i in &word_positions {
        tokens[i].1 = false;
    }
    let mut extra = 0;
    for op in diff_tokens(&expected_words, &typed_words) {
        match op {
            DiffOp::Equal { base_idx, .. } => tokens[word_positions[base_idx]].1 = true,
            DiffOp::Insert { .. } => extra += 1,
            DiffOp::Delete { .. } => {}
        }
    }

    let total = word_positions.len();
    let correct = word_positions.iter().filter(|&&i| tokens[i].1).count();
    Recall {
        tokens,
        correct,
        total,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOHN_3_16: &str = "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.";

    fn hidden_positions(text: &str) -> Vec<usize> {
        text.split_whitespace()
            .enumerate()
            .filter(|(_, t)| t.contains('_'))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn levels_hide_progressively_more_and_nest() {
        let words = word_count(JOHN_3_16);
        assert_eq!(words, 25);
        assert_eq!(cloze(JOHN_3_16, 0, 7), JOHN_3_16);

        let mut previous: Vec<usize> = Vec::new();
        for (level, expected) in [(1u8, 7usize), (2, 13), (3, 19), (5, 25)] {
            let hidden = hidden_positions(&cloze(JOHN_3_16, level, 7));
            assert_eq!(hidden.len(), expected, "level {}", level);
            assert!(
                previous.iter().all(|p| hidden.contains(p)),
                "level {} nests",
                level
            );
            previous = hidden;
        }
    }

    #[test]
    fn cloze_is_deterministic_per_salt() {
        assert_eq!(cloze(JOHN_3_16, 2, 1), cloze(JOHN_3_16, 2, 1));
        assert_ne!(cloze(JOHN_3_16, 2, 1), cloze(JOHN_3_16, 2, 2));
    }

    #[test]
    fn blanks_and_initials_keep_punctuation() {
        assert_eq!(cloze("the world, (and", 5, 0), "___ _____, (___");
        assert_eq!(cloze("the world, (and Sarah’s LORD:", 4, 0), "t w, (a S L:");
        // Bare punctuation is never hidden or counted.
        assert_eq!(cloze("Amen — so be it", 5, 0), "____ — __ __ __");
        assert_eq!(word_count("Amen — so be it"), 4);
        // Even a short verse hides something at level 1.
        assert_eq!(hidden_positions(&cloze("Jesus wept.", 1, 0)).len(), 1);
    }

    #[test]
    fn recall_scores_words_ignoring_case_and_punctuation() {
        let perfect = score_recall(
            JOHN_3_16,
            "for god so loved the world that he gave his only begotten son that whosoever believeth in him should not perish but have everlasting life",
        );
        assert_eq!((perfect.correct, perfect.total, perfect.extra), (25, 25, 0));
        assert!(perfect.is_perfect());

        let partial = score_recall("Jesus wept.", "Jesus cried");
        assert_eq!((partial.correct, partial.total, partial.extra), (1, 2, 1));
        assert_eq!(partial.percent(), 50);
        assert_eq!(partial.tokens, vec![("Jesus", true), ("wept.", false)]);

        let empty = score_recall("Jesus wept.", "");
        assert_eq!((empty.correct, empty.percent()), (0, 0));
    }
}
