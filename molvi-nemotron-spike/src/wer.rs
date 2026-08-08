//! Pure text normalization + word-level WER. No deps.
//! ponytail: ~30 lines, std-only — no `jiwer`/`edit-distance` crate for a viability spike.

/// Lowercase, strip punctuation, collapse whitespace, trim.
/// Keeps Unicode word chars (Cyrillic etc.); drops everything in the ASCII
/// punctuation set plus the common Unicode quote/dash siblings.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || c == '\'' || c == '\u{2019}' {
            out.extend(c.to_lowercase());
        } else {
            out.push(' ');
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = true;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                collapsed.push(' ');
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

/// Word-level Levenshtein distance between two token slices.
fn word_distance(ref_words: &[&str], hyp_words: &[&str]) -> usize {
    let m = ref_words.len();
    let n = hyp_words.len();
    // ponytail: two-row rolling word-Levenshtein — O(n) memory; fine for short reference clips.
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if ref_words[i - 1] == hyp_words[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Word error rate: word-level edit distance / reference word count.
/// Returns 0.0 for an empty reference (no measurable error).
pub fn wer(reference: &str, hypothesis: &str) -> f64 {
    let r = normalize(reference);
    let h = normalize(hypothesis);
    let r_words: Vec<&str> = r.split_whitespace().collect();
    let h_words: Vec<&str> = h.split_whitespace().collect();
    if r_words.is_empty() {
        return 0.0;
    }
    word_distance(&r_words, &h_words) as f64 / r_words.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_punctuation_and_case() {
        assert_eq!(normalize("Hello, World!"), "hello world");
        assert_eq!(normalize("Привет,  это  тест."), "привет это тест");
        assert_eq!(normalize("  a-b'c  "), "a b'c");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn wer_known_pairs() {
        // Perfect match after normalization.
        assert!((wer("Hello, World!", "hello world") - 0.0).abs() < 1e-9);
        // 1 substitution of 4 words -> 0.25.
        assert!((wer("a b c d", "a x c d") - 0.25).abs() < 1e-9);
        // 1 deletion + 1 insertion of 4 ref words -> 0.5.
        assert!(
            (wer("the quick brown fox", "the brown fox jumps") - 0.5).abs() < 1e-9,
            "alignment: the=the, quick deleted, brown=brown, fox=fox, jumps inserted"
        );
        // Empty reference -> 0.0 by definition (guard).
        assert!((wer("", "anything") - 0.0).abs() < 1e-9);
    }
}
