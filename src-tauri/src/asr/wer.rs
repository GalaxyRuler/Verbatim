//! Word error rate utilities shared by Android ASR benchmark harnesses.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordErrorRate {
    pub errors: usize,
    pub reference_words: usize,
    pub hypothesis_words: usize,
    pub wer: f64,
}

pub fn word_error_rate(reference: &str, hypothesis: &str) -> WordErrorRate {
    let reference_words = normalize_words(reference);
    let hypothesis_words = normalize_words(hypothesis);
    let errors = levenshtein_distance(&reference_words, &hypothesis_words);
    let wer = if reference_words.is_empty() {
        if hypothesis_words.is_empty() {
            0.0
        } else {
            1.0
        }
    } else {
        errors as f64 / reference_words.len() as f64
    };

    WordErrorRate {
        errors,
        reference_words: reference_words.len(),
        hypothesis_words: hypothesis_words.len(),
        wer,
    }
}

pub fn normalize_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || character.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

pub fn levenshtein_distance(reference: &[String], hypothesis: &[String]) -> usize {
    if reference.is_empty() {
        return hypothesis.len();
    }
    if hypothesis.is_empty() {
        return reference.len();
    }

    let mut previous = (0..=hypothesis.len()).collect::<Vec<_>>();
    let mut current = vec![0; hypothesis.len() + 1];

    for (reference_index, reference_word) in reference.iter().enumerate() {
        current[0] = reference_index + 1;

        for (hypothesis_index, hypothesis_word) in hypothesis.iter().enumerate() {
            let insertion = current[hypothesis_index] + 1;
            let deletion = previous[hypothesis_index + 1] + 1;
            let substitution =
                previous[hypothesis_index] + usize::from(reference_word != hypothesis_word);
            current[hypothesis_index + 1] = insertion.min(deletion).min(substitution);
        }

        std::mem::swap(&mut previous, &mut current);
    }

    previous[hypothesis.len()]
}

pub fn aggregate_word_error_rates(items: &[WordErrorRate]) -> WordErrorRate {
    let errors = items.iter().map(|item| item.errors).sum::<usize>();
    let reference_words = items.iter().map(|item| item.reference_words).sum::<usize>();
    let hypothesis_words = items
        .iter()
        .map(|item| item.hypothesis_words)
        .sum::<usize>();
    let wer = if reference_words == 0 {
        if hypothesis_words == 0 {
            0.0
        } else {
            1.0
        }
    } else {
        errors as f64 / reference_words as f64
    };

    WordErrorRate {
        errors,
        reference_words,
        hypothesis_words,
        wer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_punctuation_and_whitespace() {
        assert_eq!(
            normalize_words("  Hello,   WORLD!\nSherpa. "),
            vec!["hello", "world", "sherpa"]
        );
    }

    #[test]
    fn computes_word_level_wer_for_known_pairs() {
        let exact = word_error_rate("hello world", "hello world");
        assert_eq!(exact.errors, 0);
        assert_eq!(exact.reference_words, 2);
        assert_eq!(exact.wer, 0.0);

        let one_substitution = word_error_rate("the quick brown fox", "the quick red fox");
        assert_eq!(one_substitution.errors, 1);
        assert_eq!(one_substitution.reference_words, 4);
        assert_eq!(one_substitution.wer, 0.25);

        let insertion_and_deletion = word_error_rate(
            "after early nightfall the yellow lamps",
            "after nightfall yellow lamps glow",
        );
        assert_eq!(insertion_and_deletion.errors, 3);
        assert_eq!(insertion_and_deletion.reference_words, 6);
        assert_eq!(insertion_and_deletion.wer, 0.5);
    }

    #[test]
    fn aggregates_from_total_errors_and_reference_words() {
        let items = vec![
            word_error_rate("one two three four", "one two three"),
            word_error_rate("alpha beta", "alpha gamma"),
        ];

        let aggregate = aggregate_word_error_rates(&items);

        assert_eq!(aggregate.errors, 2);
        assert_eq!(aggregate.reference_words, 6);
        assert_eq!(aggregate.hypothesis_words, 5);
        assert!((aggregate.wer - (2.0 / 6.0)).abs() < f64::EPSILON);
    }
}
