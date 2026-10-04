//! Word-level diff (an LCS over whitespace-separated tokens), shared by
//! `bible diff` and the recall scoring in `bible memorize --quiz`.

/// One token-level edit from a base text to another.
pub enum DiffOp<'a> {
    /// A token both sides share. `text` is the other side's surface form
    /// (punctuation and case may differ); `base_idx` is its base position.
    Equal { base_idx: usize, text: &'a str },
    /// A token only in the other text.
    Insert { text: &'a str },
    /// A token only in the base text.
    Delete { text: &'a str },
}

impl DiffOp<'_> {
    /// The op's name in JSON/TSV output.
    pub fn name(&self) -> &'static str {
        match self {
            DiffOp::Equal { .. } => "equal",
            DiffOp::Insert { .. } => "insert",
            DiffOp::Delete { .. } => "delete",
        }
    }

    pub fn text(&self) -> &str {
        match self {
            DiffOp::Equal { text, .. } | DiffOp::Insert { text } | DiffOp::Delete { text } => text,
        }
    }
}

/// Case- and punctuation-insensitive token key, so "world," matches "World".
pub fn token_key(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Token-level LCS diff from `base` to `other`. Equal ops carry the other
/// text's surface form plus the base index.
pub fn diff_tokens<'a>(base: &[&'a str], other: &[&'a str]) -> Vec<DiffOp<'a>> {
    let base_keys: Vec<String> = base.iter().map(|t| token_key(t)).collect();
    let other_keys: Vec<String> = other.iter().map(|t| token_key(t)).collect();
    let (n, m) = (base.len(), other.len());

    let idx = |i: usize, j: usize| i * (m + 1) + j;
    let mut dp = vec![0usize; (n + 1) * (m + 1)];
    for i in 1..=n {
        for j in 1..=m {
            dp[idx(i, j)] = if base_keys[i - 1] == other_keys[j - 1] {
                dp[idx(i - 1, j - 1)] + 1
            } else {
                dp[idx(i - 1, j)].max(dp[idx(i, j - 1)])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && base_keys[i - 1] == other_keys[j - 1] {
            ops.push(DiffOp::Equal {
                base_idx: i - 1,
                text: other[j - 1],
            });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[idx(i, j - 1)] >= dp[idx(i - 1, j)]) {
            ops.push(DiffOp::Insert { text: other[j - 1] });
            j -= 1;
        } else {
            ops.push(DiffOp::Delete { text: base[i - 1] });
            i -= 1;
        }
    }
    ops.reverse();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops_summary(base: &str, other: &str) -> Vec<(char, String)> {
        let base_tokens: Vec<&str> = base.split_whitespace().collect();
        let other_tokens: Vec<&str> = other.split_whitespace().collect();
        diff_tokens(&base_tokens, &other_tokens)
            .iter()
            .map(|op| match op {
                DiffOp::Equal { text, .. } => ('=', text.to_string()),
                DiffOp::Insert { text } => ('+', text.to_string()),
                DiffOp::Delete { text } => ('-', text.to_string()),
            })
            .collect()
    }

    #[test]
    fn diff_identical_text_is_all_equal() {
        let ops = ops_summary("For God so loved", "For God so loved");
        assert!(ops.iter().all(|(op, _)| *op == '='));
        assert_eq!(ops.len(), 4);
    }

    #[test]
    fn diff_marks_insertions_and_deletions() {
        let ops = ops_summary("the only begotten Son", "the only Son");
        assert_eq!(
            ops,
            vec![
                ('=', "the".to_string()),
                ('=', "only".to_string()),
                ('-', "begotten".to_string()),
                ('=', "Son".to_string()),
            ]
        );

        let ops = ops_summary("he gave", "he freely gave");
        assert_eq!(
            ops,
            vec![
                ('=', "he".to_string()),
                ('+', "freely".to_string()),
                ('=', "gave".to_string()),
            ]
        );
    }

    #[test]
    fn diff_ignores_case_and_punctuation_for_matching() {
        // "world," matches "World" — equal ops keep the other's surface form.
        let ops = ops_summary("the world, he", "the World he");
        assert!(ops.iter().all(|(op, _)| *op == '='));
        assert_eq!(ops[1].1, "World");
    }

    #[test]
    fn diff_handles_empty_sides() {
        assert!(ops_summary("", "").is_empty());
        assert!(ops_summary("a b", "").iter().all(|(op, _)| *op == '-'));
        assert!(ops_summary("", "a b").iter().all(|(op, _)| *op == '+'));
    }

    #[test]
    fn op_names_and_text() {
        let base = ["a", "b"];
        let other = ["a", "c"];
        let ops = diff_tokens(&base, &other);
        let named: Vec<(&str, &str)> = ops.iter().map(|op| (op.name(), op.text())).collect();
        assert_eq!(named, [("equal", "a"), ("delete", "b"), ("insert", "c")]);
    }
}
