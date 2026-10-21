//! Generic text operations.

/// The number of characters in a string.
///
/// `str::len` counts **bytes**, which is the wrong measure for a length limit
/// that a user reads in characters: a name of 60 emoji is 240 bytes and 60
/// characters. Every length rule in the API is expressed in characters.
pub fn char_len(value: &str) -> usize {
    value.chars().count()
}

/// Whether a string is empty or contains only whitespace.
pub fn is_blank(value: &str) -> bool {
    value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_len_counts_characters_not_bytes() {
        assert_eq!(char_len("hello"), 5);
        assert_eq!(char_len("naïve"), 5);
        assert_eq!(char_len("🦀🦀"), 2);
        assert_eq!("naïve".len(), 6);
    }

    #[test]
    fn is_blank_ignores_whitespace() {
        assert!(is_blank(""));
        assert!(is_blank("   \t\n"));
        assert!(!is_blank(" x "));
    }
}
