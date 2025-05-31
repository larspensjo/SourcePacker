/*
 * This module provides utilities for tokenizing text content.
 * It includes a simple whitespace-based token estimation and a more
 * accurate estimation using the tiktoken-rs library (cl100k_base model).
 */

use tiktoken_rs::{CoreBPE, cl100k_base};
// Import log macros for error logging
use log::error;

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

/*
 * Estimates the number of tokens in a given string using the `cl100k_base`
 * model from the `tiktoken-rs` library. This model is commonly used by
 * OpenAI's GPT-3.5 and GPT-4 models.
 *
 * If the BPE model fails to initialize, an error is logged, and the function
 * returns 0. This function is designed to be relatively safe, preferring to
 * underestimate or return zero on failure rather than panicking. The error
 * type from `cl100k_base()` is `tiktoken_rs::TiktokenError`.
 *
 * Args:
 *   content: A string slice containing the text to be tokenized.
 *
 * Returns:
 *   The estimated number of tokens according to `cl100k_base`, or 0 if
 *   an error occurs during BPE initialization.
 */
pub fn estimate_tokens_tiktoken(content: &str) -> usize {
    match cl100k_base() {
        Ok(bpe) => bpe.encode_with_special_tokens(content).len(),
        Err(e) => {
            // The type of 'e' is inferred as tiktoken_rs::TiktokenError here.
            error!(
                "Failed to initialize TikToken BPE (cl100k_base): {:?}. Token count will be 0.",
                e
            );
            0
        }
    }
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

    // --- Tests for estimate_tokens_tiktoken ---

    #[test]
    fn test_estimate_tokens_tiktoken_empty_string() {
        assert_eq!(estimate_tokens_tiktoken(""), 0);
    }

    #[test]
    fn test_estimate_tokens_tiktoken_simple_text() {
        // Note: Exact token counts can vary with library versions or model details.
        // This is a representative example.
        // "hello world" is typically 2 tokens with cl100k_base.
        assert_eq!(estimate_tokens_tiktoken("hello world"), 2);
    }

    #[test]
    fn test_estimate_tokens_tiktoken_text_with_punctuation() {
        // "Hello, world!" is typically 3 tokens: "Hello", ",", " world!" (or similar breakdown)
        assert_eq!(estimate_tokens_tiktoken("Hello, world!"), 3);
    }

    #[test]
    fn test_estimate_tokens_tiktoken_text_with_newline() {
        // "tiktoken is great\nfun" - this example is from tiktoken_rs docs
        // Expected: "tiktoken" " is" " great" "\n" "fun" -> 5 tokens
        assert_eq!(estimate_tokens_tiktoken("tiktoken is great\nfun"), 5);
    }

    #[test]
    fn test_estimate_tokens_tiktoken_longer_phrase() {
        // A more complex phrase to ensure it handles typical text.
        // "This is a test sentence for the tokenizer."
        // Expected: "This" " is" " a" " test" " sentence" " for" " the" " token" "izer" "." -> 10 tokens
        assert_eq!(
            estimate_tokens_tiktoken("This is a test sentence for the tokenizer."),
            10
        );
    }

    #[test]
    fn test_estimate_tokens_tiktoken_special_characters_and_numbers() {
        // Example from OpenAI cookbook: "antidisestablishmentarianism" -> 5 tokens
        assert_eq!(estimate_tokens_tiktoken("antidisestablishmentarianism"), 5);
        // "ਤੁਹਾਡਾ ਸੁਆਗਤ ਹੈ" (Punjabi "Welcome") -> typically more tokens due to script
        // For cl100k_base, this specific phrase often breaks down into multiple byte tokens.
        // "ਤੁ" "ਹਾ" "ਡਾ" " ਸੁ" "ਆ" "ਗਤ" " ਹੈ" -> 7 tokens
        assert_eq!(estimate_tokens_tiktoken("ਤੁਹਾਡਾ ਸੁਆਗਤ ਹੈ"), 7);
    }
}
