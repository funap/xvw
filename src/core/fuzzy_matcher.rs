// Port of funap/fuzzy-matcher (https://github.com/funap/fuzzy-matcher)
// MIT License - Copyright (c) 2019-2026 funap

/// Scoring constants ported from funap/fuzzy-matcher.
pub mod scoring {
    pub const CHARACTER_MATCH_BONUS: i32 = 1;
    pub const SAME_CASE_BONUS: i32 = 1;
    pub const FIRST_LETTER_BONUS: i32 = 8;
    pub const CONSECUTIVE_MATCH_BONUS: i32 = 5;
    pub const START_OF_EXTENSION_BONUS: i32 = 3;
    pub const CAMEL_CASE_BONUS: i32 = 4;
    pub const SEPARATOR_BONUS: i32 = 4;
    pub const DIRECTORY_SEPARATOR_BONUS: i32 = 5;
}

#[inline]
fn char_equal_case_insensitive(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(&b);
    }
    a.to_lowercase().eq(b.to_lowercase())
}

#[inline]
fn calculate_score(pattern_char: char, target_chars: &[char], target_index: usize, matches_sequence_length: i32) -> i32 {
    let target_char = target_chars[target_index];
    if !char_equal_case_insensitive(pattern_char, target_char) {
        return 0;
    }

    let mut score = scoring::CHARACTER_MATCH_BONUS;

    if matches_sequence_length > 0 {
        score += matches_sequence_length * scoring::CONSECUTIVE_MATCH_BONUS;
    }

    if pattern_char == target_char {
        score += scoring::SAME_CASE_BONUS;
    }

    if target_index == 0 {
        score += scoring::FIRST_LETTER_BONUS;
    } else {
        match target_chars[target_index - 1] {
            '\\' | '/' => {
                score += scoring::DIRECTORY_SEPARATOR_BONUS;
            }
            ' ' | '_' => {
                score += scoring::SEPARATOR_BONUS;
            }
            '.' => {
                score += scoring::START_OF_EXTENSION_BONUS;
            }
            prev => {
                if prev.is_lowercase() && target_char.is_uppercase() {
                    score += scoring::CAMEL_CASE_BONUS;
                }
            }
        }
    }

    score
}

/// Dynamic programming fuzzy string matcher ported from C++ library `funap/fuzzy-matcher`.
#[derive(Debug, Clone, Default)]
pub struct FuzzyMatcher {
    pattern: Vec<char>,
    score_matrix: Vec<i32>,
    match_matrix: Vec<i32>,
}

impl FuzzyMatcher {
    /// Creates a new `FuzzyMatcher` with the specified search pattern.
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.chars().collect(),
            score_matrix: Vec::new(),
            match_matrix: Vec::new(),
        }
    }

    /// Updates the pattern used by this matcher.
    #[allow(dead_code)]
    pub fn set_pattern(&mut self, pattern: &str) {
        self.pattern.clear();
        self.pattern.extend(pattern.chars());
    }

    /// Returns the length of the pattern in characters.
    #[allow(dead_code)]
    pub fn pattern_len(&self) -> usize {
        self.pattern.len()
    }

    /// Evaluates `target` against the pattern and returns the match score if matched.
    #[allow(dead_code)]
    pub fn score_match(&mut self, target: &str) -> Option<i32> {
        self.score_match_with_positions(target).map(|(score, _)| score)
    }

    /// Evaluates `target` against the pattern and returns the match score and
    /// matched UTF-8 byte offsets if matched.
    pub fn score_match_with_positions(&mut self, target: &str) -> Option<(i32, Vec<usize>)> {
        if self.pattern.is_empty() || target.is_empty() {
            return None;
        }

        let target_chars: Vec<char> = target.chars().collect();
        let pattern_len = self.pattern.len();
        let target_len = target_chars.len();

        if pattern_len > target_len {
            return None;
        }

        let matrix_size = pattern_len * target_len;
        self.score_matrix.clear();
        self.score_matrix.resize(matrix_size, 0);
        self.match_matrix.clear();
        self.match_matrix.resize(matrix_size, 0);

        for pattern_index in 0..pattern_len {
            let pattern_is_first = pattern_index == 0;
            let pattern_offset = pattern_index * target_len;
            let prev_pattern_offset = if pattern_is_first { 0 } else { pattern_offset - target_len };

            for target_index in 0..target_len {
                let target_is_first = target_index == 0;
                let current_index = pattern_offset + target_index;
                let left_index = if target_is_first { 0 } else { current_index - 1 };
                let diag_index = if pattern_is_first || target_is_first {
                    0
                } else {
                    prev_pattern_offset + (target_index - 1)
                };

                let left_score = if target_is_first { 0 } else { self.score_matrix[left_index] };
                let diag_score = if pattern_is_first || target_is_first {
                    0
                } else {
                    self.score_matrix[diag_index]
                };
                let matches_sequence_length = if pattern_is_first || target_is_first {
                    0
                } else {
                    self.match_matrix[diag_index]
                };

                let score = if diag_score == 0 && !pattern_is_first {
                    0
                } else {
                    calculate_score(self.pattern[pattern_index], &target_chars, target_index, matches_sequence_length)
                };

                if score > 0 && left_score <= diag_score + score {
                    self.match_matrix[current_index] = matches_sequence_length + 1;
                    self.score_matrix[current_index] = diag_score + score;
                } else {
                    self.match_matrix[current_index] = 0;
                    self.score_matrix[current_index] = left_score;
                }
            }
        }

        let result = self.score_matrix[matrix_size - 1];
        if result <= 0 {
            return None;
        }

        // Restore match positions from bottom-right back to top-left
        let mut char_positions = Vec::with_capacity(pattern_len);
        let mut pattern_idx = pattern_len - 1;
        let mut target_idx = target_len - 1;

        loop {
            let current_index = pattern_idx * target_len + target_idx;
            let m = self.match_matrix[current_index];

            if m == 0 {
                if target_idx > 0 {
                    target_idx -= 1;
                } else {
                    break;
                }
            } else {
                char_positions.push(target_idx);
                if pattern_idx > 0 && target_idx > 0 {
                    pattern_idx -= 1;
                    target_idx -= 1;
                } else {
                    break;
                }
            }
        }
        char_positions.reverse();

        // Map character positions back to UTF-8 byte offsets
        let mut byte_offsets = Vec::with_capacity(target_len);
        for (offset, _) in target.char_indices() {
            byte_offsets.push(offset);
        }

        let byte_positions: Vec<usize> = char_positions.into_iter().filter_map(|pos| byte_offsets.get(pos).copied()).collect();

        Some((result, byte_positions))
    }
}

/// Convenience function that performs a one-off fuzzy match.
#[allow(dead_code)]
pub fn fuzzy_match(pattern: &str, target: &str) -> Option<(i32, Vec<usize>)> {
    let mut matcher = FuzzyMatcher::new(pattern);
    matcher.score_match_with_positions(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_matching() {
        let mut matcher = FuzzyMatcher::new("abc");

        // Exact match
        let (score, pos) = matcher.score_match_with_positions("abc").expect("match expected");
        assert!(score > 0);
        assert_eq!(pos, vec![0, 1, 2]);

        // Partial match
        let (score, pos) = matcher.score_match_with_positions("xaxbxc").expect("match expected");
        assert!(score > 0);
        assert_eq!(pos, vec![1, 3, 5]);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let mut matcher = FuzzyMatcher::new("abc");
        assert!(matcher.score_match("ABC").unwrap() > 0);
        assert!(matcher.score_match("aBc").unwrap() > 0);
    }

    #[test]
    fn test_bonus_scoring() {
        // First letter bonus
        let mut matcher_a = FuzzyMatcher::new("a");
        let score_start = matcher_a.score_match("abc").unwrap();
        let score_mid = matcher_a.score_match("bac").unwrap();
        assert!(score_start > score_mid);

        // Camel case bonus
        let mut matcher_c = FuzzyMatcher::new("c");
        let score_camel = matcher_c.score_match("testCase").unwrap();
        let score_normal = matcher_c.score_match("testcase").unwrap();
        assert!(score_camel > score_normal);
    }

    #[test]
    fn test_edge_cases() {
        let mut matcher_empty = FuzzyMatcher::new("");
        assert!(matcher_empty.score_match("test").is_none());

        let mut matcher_test = FuzzyMatcher::new("test");
        assert!(matcher_test.score_match("").is_none());

        let mut matcher_toolong = FuzzyMatcher::new("toolong");
        assert!(matcher_toolong.score_match("short").is_none());
    }

    #[test]
    fn test_separator_bonus() {
        let mut matcher_f = FuzzyMatcher::new("f");
        let score_sep_bs = matcher_f.score_match("test\\file").unwrap();
        let score_normal = matcher_f.score_match("testfile").unwrap();
        assert!(score_sep_bs > score_normal);

        let score_sep_slash = matcher_f.score_match("test/file").unwrap();
        assert!(score_sep_slash > score_normal);

        let mut matcher_t = FuzzyMatcher::new("t");
        let score_sep_us = matcher_t.score_match("some_test").unwrap();
        let score_normal_t = matcher_t.score_match("sometest").unwrap();
        assert!(score_sep_us > score_normal_t);
    }

    #[test]
    fn test_consecutive_matches() {
        let mut matcher = FuzzyMatcher::new("abc");
        let score_consecutive = matcher.score_match("abc").unwrap();
        let score_non_consecutive = matcher.score_match("axbxc").unwrap();
        assert!(score_consecutive > score_non_consecutive);
    }

    #[test]
    fn test_utf8_japanese_matching() {
        let mut matcher = FuzzyMatcher::new("てすと");
        let (score, pos) = matcher.score_match_with_positions("あていすうと").expect("japanese match");
        assert!(score > 0);
        // て: byte 3, す: byte 9, と: byte 15
        assert_eq!(pos, vec![3, 9, 15]);
    }

    #[test]
    fn test_scenario_acronym_fmt() {
        let mut matcher = FuzzyMatcher::new("fmt");

        let s1 = matcher.score_match("file_management_tool.cpp").unwrap();
        let s2 = matcher.score_match("FastMessageThread.cpp").unwrap();
        let s3 = matcher.score_match("format_table.cpp").unwrap();

        assert_eq!(s1, 22);
        assert_eq!(s2, 19);
        assert_eq!(s3, 18);
        assert!(s1 > s2 && s2 > s3);
    }

    #[test]
    fn test_scenario_consecutive_str() {
        let mut matcher = FuzzyMatcher::new("str");

        let s1 = matcher.score_match("StringStream.cpp").unwrap();
        let s2 = matcher.score_match("parse_structure.cpp").unwrap();
        let s3 = matcher.score_match("system_trace.cpp").unwrap();

        assert_eq!(s1, 28);
        assert_eq!(s2, 25);
        assert_eq!(s3, 23);
        assert!(s1 > s2 && s2 > s3);
    }

    #[test]
    fn test_scenario_optimal_boundary_latest_tests() {
        let mut matcher = FuzzyMatcher::new("test");
        let (score, pos) = matcher.score_match_with_positions("latest_tests").unwrap();

        // Should prefer `_[test]s` (indices 7, 8, 9, 10) with boundary bonus (42 pts) over `la[test]` (38 pts)
        assert_eq!(score, 42);
        assert_eq!(pos, vec![7, 8, 9, 10]);
    }
}
