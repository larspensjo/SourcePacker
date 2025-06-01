/*
 * This module provides utilities for token counting.
 * It defines an abstraction `TokenCounterOperations` for counting tokens in a string,
 * and concrete implementations: `CoreTikTokenCounter` that uses the `tiktoken-rs`
 * library and `SimpleWhitespaceTokenCounter` for a basic word count.
 * This allows for decoupling the token counting logic from its consumers and facilitates
 * easier testing and strategy selection.
 */
use tiktoken_rs::cl100k_base;
// Import log macros for error logging
use log::error;

/*
 * Defines the contract for a service that can count tokens in a given text string.
 * Implementations of this trait will provide specific tokenization strategies.
 */
pub trait TokenCounterOperations: Send + Sync {
    /*
     * Counts the number of tokens in the provided text.
     * The definition of a "token" depends on the underlying implementation.
     */
    fn count_tokens(&self, text: &str) -> usize;
}

/*
 * A concrete implementation of `TokenCounterOperations` that uses the `tiktoken-rs`
 * library with the "cl100k_base" model for tokenization. This model is commonly
 * used by OpenAI's GPT-3.5 and GPT-4 models.
 */
pub struct CoreTikTokenCounter;

impl CoreTikTokenCounter {
    /*
     * Creates a new instance of `CoreTikTokenCounter`.
     */
    pub fn new() -> Self {
        CoreTikTokenCounter
    }
}

impl TokenCounterOperations for CoreTikTokenCounter {
    /*
     * Estimates the number of tokens in a given string using the `cl100k_base`
     * model from the `tiktoken-rs` library.
     *
     * If the BPE model fails to initialize, an error is logged, and the function
     * falls back to a simple whitespace split count. This approach ensures that
     * token counting remains functional, albeit less accurate, in case of BPE
     * initialization issues.
     *
     * Args:
     *   content: A string slice containing the text to be tokenized.
     *
     * Returns:
     *   The estimated number of tokens according to `cl100k_base`, or a whitespace-based
     *   count if an error occurs during BPE initialization.
     */
    fn count_tokens(&self, text: &str) -> usize {
        match cl100k_base() {
            Ok(bpe) => bpe.encode_with_special_tokens(text).len(),
            Err(e) => {
                error!(
                    "Failed to initialize TikToken BPE (cl100k_base): {:?}. Falling back to whitespace token count.",
                    e
                );
                // Fallback to simple whitespace counting if tiktoken fails
                text.split_whitespace().count()
            }
        }
    }
}

/*
 * A concrete implementation of `TokenCounterOperations` that estimates tokens
 * by counting words separated by whitespace. This is a very basic estimation.
 */
pub struct SimpleWhitespaceTokenCounter;

impl SimpleWhitespaceTokenCounter {
    /*
     * Creates a new instance of `SimpleWhitespaceTokenCounter`.
     */
    pub fn new() -> Self {
        SimpleWhitespaceTokenCounter
    }
}

impl TokenCounterOperations for SimpleWhitespaceTokenCounter {
    /*
     * Estimates the number of tokens in a given string by counting words
     * separated by whitespace.
     *
     * Args:
     *   content: A string slice containing the text to be tokenized.
     *
     * Returns:
     *   The estimated number of tokens (words) in the content.
     */
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Tests for SimpleWhitespaceTokenCounter ---
    #[test]
    fn test_simple_whitespace_counter_empty_string() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_simple_whitespace_counter_single_word() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello"), 1);
    }

    #[test]
    fn test_simple_whitespace_counter_multiple_words() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello world example"), 3);
    }

    #[test]
    fn test_simple_whitespace_counter_leading_trailing_spaces() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("  hello world  "), 2);
    }

    #[test]
    fn test_simple_whitespace_counter_multiple_spaces_between_words() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello   world   example"), 3);
    }

    #[test]
    fn test_simple_whitespace_counter_with_punctuation() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello, world! example."), 3);
    }

    #[test]
    fn test_simple_whitespace_counter_with_newlines() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello\nworld\nexample"), 3);
    }

    #[test]
    fn test_simple_whitespace_counter_mixed_whitespace() {
        let counter = SimpleWhitespaceTokenCounter::new();
        assert_eq!(counter.count_tokens("hello\tworld\r\nexample"), 3);
    }

    // --- Tests for CoreTikTokenCounter ---

    #[test]
    fn test_core_tiktoken_counter_empty_string() {
        let counter = CoreTikTokenCounter::new();
        assert_eq!(counter.count_tokens(""), 0);
    }

    #[test]
    fn test_core_tiktoken_counter_simple_text() {
        let counter = CoreTikTokenCounter::new();
        // "hello world" is typically 2 tokens with cl100k_base.
        assert_eq!(counter.count_tokens("hello world"), 2);
    }

    #[test]
    fn test_core_tiktoken_counter_text_with_punctuation() {
        let counter = CoreTikTokenCounter::new();
        // "Hello, world!" is typically 4 tokens: "Hello", ",", "world", "!" (or similar breakdown)
        assert_eq!(counter.count_tokens("Hello, world!"), 4);
    }

    #[test]
    fn test_core_tiktoken_counter_text_with_newline() {
        let counter = CoreTikTokenCounter::new();
        // "tiktoken is great\nfun" - this example is from tiktoken_rs docs
        // Expected: "tiktoken" " is" " great" "\n" "fun" -> 5 tokens
        assert_eq!(counter.count_tokens("tiktoken is great\nfun"), 5);
    }

    #[test]
    fn test_core_tiktoken_counter_longer_phrase() {
        let counter = CoreTikTokenCounter::new();
        // A more complex phrase to ensure it handles typical text.
        // "This is a test sentence for the tokenizer."
        // Expected: "This" " is" " a" " test" " sentence" " for" " the" " tokenizer" "." -> 9 tokens
        assert_eq!(
            counter.count_tokens("This is a test sentence for the tokenizer."),
            9
        );
    }

    #[test]
    fn test_core_tiktoken_counter_special_characters_and_numbers() {
        let counter = CoreTikTokenCounter::new();
        // Example from OpenAI cookbook: "antidisestablishmentarianism" -> 5 tokens
        assert_eq!(counter.count_tokens("antidisestablishmentarianism"), 5);
        // "ਤੁਹਾਡਾ ਸੁਆਗਤ ਹੈ" (Punjabi "Welcome") -> typically more tokens due to script
        // For cl100k_base, this specific phrase often breaks down into multiple byte tokens.
        // "ਤੁ" "ਹਾ" "ਡਾ" " ਸੁ" "ਆ" "ਗਤ" " ਹੈ" -> 7 tokens
        assert_eq!(counter.count_tokens("ਤੁਹਾਡਾ ਸੁਆਗਤ ਹੈ"), 7);
    }
}
