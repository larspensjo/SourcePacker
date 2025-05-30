/*
 * This module provides utilities for tokenizing text content.
 * Initially, it offers a simple whitespace-based token estimation,
 * with plans to integrate more sophisticated tokenizers like tiktoken-rs.
 */

/*
 * Estimates the number of tokens in a given string by counting words
 * separated by whitespace. This is a very basic estimation and does not
 * reflect the actual token count used by advanced language models.
 *
 * Args:
 *   content: A string slice containing the text to be tokenized.
 *
 * Returns:
 *   The estimated number of tokens (words) in the content.
 */
pub fn estimate_tokens_simple_whitespace(content: &str) -> usize {
    content.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_simple_whitespace_empty_string() {
        assert_eq!(estimate_tokens_simple_whitespace(""), 0);
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_single_word() {
        assert_eq!(estimate_tokens_simple_whitespace("hello"), 1);
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_multiple_words() {
        assert_eq!(estimate_tokens_simple_whitespace("hello world example"), 3);
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_leading_trailing_spaces() {
        assert_eq!(estimate_tokens_simple_whitespace("  hello world  "), 2);
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_multiple_spaces_between_words() {
        assert_eq!(
            estimate_tokens_simple_whitespace("hello   world   example"),
            3
        );
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_with_punctuation() {
        assert_eq!(
            estimate_tokens_simple_whitespace("hello, world! example."),
            3
        );
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_with_newlines() {
        assert_eq!(
            estimate_tokens_simple_whitespace("hello\nworld\nexample"),
            3
        );
    }

    #[test]
    fn test_estimate_tokens_simple_whitespace_mixed_whitespace() {
        assert_eq!(
            estimate_tokens_simple_whitespace("hello\tworld\r\nexample"),
            3
        );
    }
}
