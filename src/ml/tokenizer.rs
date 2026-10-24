//! The WordPiece tokenizer for `all-MiniLM-L6-v2`.
//!
//! A transformer encoder does not receive text: it receives the token ids the
//! vocabulary assigns. This module is the only place in the crate that turns a
//! note into those ids, and it is deliberately hand-written, because
//! [`docs/features/ml/architecture.md`](../../docs/features/ml/architecture.md)
//! §12.3 refuses a tokenizer crate for one fixed vocabulary: it would add a
//! dependency and a build-time cost for a 150-line algorithm.
//!
//! The algorithm is BERT's **uncased** WordPiece:
//!
//! 1. **Normalise.** Lower-case, strip accents, replace whitespace with a single
//!    space and remove control characters.
//! 2. **Basic tokenise.** Split on whitespace, and split punctuation into its own
//!    tokens.
//! 3. **WordPiece.** Greedy longest-match-first against the vocabulary; a
//!    continuation piece is prefixed with `##`; a word that cannot be covered is
//!    `[UNK]`; a word over 100 characters is `[UNK]`.
//! 4. **Frame.** `[CLS]` + pieces + `[SEP]`, truncated so the sequence never
//!    exceeds [`MAX_TOKENS`].
//!
//! The vocabulary is the `vocab.txt` described by
//! [`ADR-0008`](../../docs/decisions/ADR-0008-local-pure-rust-ml.md) §10: one
//! token per line, and the line index **is** the token id. Nothing else is
//! inferred, so a vocabulary that lacks `[UNK]`, `[CLS]` or `[SEP]` is refused at
//! construction rather than producing sequences the model cannot read.
//!
//! # Accent stripping is a mapping table, not Unicode NFD
//!
//! `std` has no Unicode normalisation. The mapping below covers the Latin-1
//! Supplement and Latin Extended-A letters that occur in practice, and it drops
//! combining diacritical marks. A character outside that set is lower-cased and
//! kept, so it reaches WordPiece unchanged and becomes `[UNK]` when the
//! vocabulary does not contain it. That is a deliberate, documented
//! approximation, not a silent one.

use std::collections::HashMap;
use std::path::Path;

use super::embedder::MlError;

/// The longest sequence this tokenizer produces, `[CLS]` and `[SEP]` included.
///
/// The architecture contract fixes 256 for this feature (§12.3), which is half
/// of the model's 512-position table. It bounds the O(tokens²) attention cost of
/// one inference, and with it the request budget.
pub const MAX_TOKENS: usize = 256;

/// The longest word, in characters, that WordPiece will try to split.
///
/// A tokenizer that walked a pathological input would turn a note into a very
/// long loop. Hugging Face's `BertTokenizer` uses the same bound; at 100
/// characters a word becomes `[UNK]` instead.
pub const MAX_WORD_CHARS: usize = 100;

/// One encoded text: the ids the encoder consumes and the mask it pools with.
///
/// The mask is `1.0` for every token that is a real piece and `0.0` for padding.
/// This tokenizer never pads — one text is embedded per call — so every element
/// is `1.0`. It is still produced, because masked mean pooling is defined
/// against it and a future batch path must not have to invent it.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenizedText {
    /// The token ids, `[CLS]` first and `[SEP]` last.
    pub ids: Vec<i32>,
    /// One element per id: `1.0` for a real token, `0.0` for padding.
    pub attention_mask: Vec<f32>,
}

/// A WordPiece vocabulary and the four special ids it must contain.
#[derive(Debug, Clone)]
pub struct WordPieceTokenizer {
    vocabulary: HashMap<String, i32>,
    unknown_id: i32,
    cls_id: i32,
    sep_id: i32,
}

impl WordPieceTokenizer {
    /// Loads a vocabulary from `vocab.txt`.
    ///
    /// # Errors
    ///
    /// Returns [`MlError::Model`] when the file cannot be read, is empty, or
    /// does not contain `[UNK]`, `[CLS]` or `[SEP]`. The message names the
    /// artifact, never a note: a vocabulary is operator input, not user data.
    pub fn from_file(path: &Path) -> Result<Self, MlError> {
        let bytes = std::fs::read(path).map_err(|error| MlError::Model {
            message: format!("cannot read the tokenizer vocabulary: {error}"),
        })?;

        let text = String::from_utf8(bytes).map_err(|error| MlError::Model {
            message: format!("the tokenizer vocabulary is not UTF-8: {error}"),
        })?;

        Self::from_vocabulary(text)
    }

    /// Builds a tokenizer from the contents of a `vocab.txt`.
    ///
    /// The line index is the token id. A trailing newline is not a token, and a
    /// Windows line ending is not part of the token, so both are removed here
    /// rather than by every caller.
    ///
    /// # Errors
    ///
    /// Returns [`MlError::Model`] when the vocabulary has no tokens or lacks one
    /// of the three special tokens the framing needs.
    pub fn from_vocabulary(text: impl Into<String>) -> Result<Self, MlError> {
        let text: String = text.into();
        let mut vocabulary = HashMap::new();
        let lines: Vec<&str> = text.split('\n').collect();
        let last = lines.len().saturating_sub(1);

        for (index, line) in lines.into_iter().enumerate() {
            let token = line.strip_suffix('\r').unwrap_or(line);
            // The final element after a trailing newline is empty and is not a
            // token. An empty line in the middle would be a corrupt vocabulary,
            // and mapping it would give an id no text can produce.
            if token.is_empty() && index == last {
                continue;
            }

            let id = i32::try_from(index).map_err(|_| MlError::Model {
                message: "the tokenizer vocabulary is larger than 2^31 tokens".to_string(),
            })?;
            vocabulary.insert(token.to_string(), id);
        }

        let unknown_id = special(&vocabulary, "[UNK]")?;
        let cls_id = special(&vocabulary, "[CLS]")?;
        let sep_id = special(&vocabulary, "[SEP]")?;

        Ok(Self {
            vocabulary,
            unknown_id,
            cls_id,
            sep_id,
        })
    }

    /// Builds a stand-in vocabulary with the four special tokens and `size`
    /// filler pieces.
    ///
    /// Used only by [`super::burn_embedder::BurnEmbedder::synthetic`], which the
    /// frozen `atom-ml-spike` binary calls to measure the Burn code path with no
    /// model file. No real word matches a filler piece, so every input becomes
    /// `[UNK]`; the *shapes* the encoder sees are real, and the vectors are
    /// meaningless, which the spike reports.
    #[must_use]
    pub fn synthetic(size: usize) -> Self {
        let mut tokens = vec![
            "[PAD]".to_string(),
            "[UNK]".to_string(),
            "[CLS]".to_string(),
            "[SEP]".to_string(),
            "[MASK]".to_string(),
        ];
        tokens.resize(size.max(5), "##synthetic".to_string());

        let mut vocabulary = HashMap::with_capacity(tokens.len());
        for (index, token) in tokens.into_iter().enumerate() {
            // The index fits in `i32` for any size this helper is asked for; a
            // caller that asked for 2^31 tokens would get the last valid id
            // rather than a panic.
            vocabulary.insert(token, i32::try_from(index).unwrap_or(i32::MAX));
        }

        Self {
            unknown_id: 1,
            cls_id: 2,
            sep_id: 3,
            vocabulary,
        }
    }

    /// The id of `[UNK]`.
    #[must_use]
    pub fn unknown_id(&self) -> i32 {
        self.unknown_id
    }

    /// The id of `[CLS]`.
    #[must_use]
    pub fn cls_id(&self) -> i32 {
        self.cls_id
    }

    /// The id of `[SEP]`.
    #[must_use]
    pub fn sep_id(&self) -> i32 {
        self.sep_id
    }

    /// The size of the vocabulary, which is the number of rows the embedding
    /// table must have.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        self.vocabulary.len()
    }

    /// Encodes one text.
    ///
    /// The result is never empty: an empty or all-unknown input still produces
    /// `[CLS] [SEP]`, which is a valid two-token sequence for the encoder.
    #[must_use]
    pub fn tokenize(&self, text: &str) -> TokenizedText {
        let mut ids = Vec::with_capacity(MAX_TOKENS);
        ids.push(self.cls_id);

        // Leave room for `[SEP]`. The bound is on the whole sequence, not on the
        // pieces, so a 300-word note cannot produce 300 tokens.
        let budget = MAX_TOKENS.saturating_sub(2);

        for piece in basic_tokens(&normalise(text)) {
            for id in self.word_pieces(&piece) {
                if ids.len() > budget {
                    break;
                }
                ids.push(id);
            }

            if ids.len() > budget {
                break;
            }
        }

        ids.push(self.sep_id);

        let attention_mask = vec![1.0_f32; ids.len()];
        TokenizedText {
            ids,
            attention_mask,
        }
    }

    /// Splits one word into WordPiece ids, greedy longest-match-first.
    ///
    /// A word that cannot be covered, or one that is too long to be worth
    /// trying, is the single `[UNK]` id. The caller never sees a partial
    /// tokenisation: BERT's rule is all-or-nothing per word, and a partial one
    /// would change what the model reads.
    fn word_pieces(&self, word: &str) -> Vec<i32> {
        let characters: Vec<char> = word.chars().collect();

        if characters.is_empty() {
            return Vec::new();
        }

        if characters.len() > MAX_WORD_CHARS {
            return vec![self.unknown_id];
        }

        let mut pieces = Vec::new();
        let mut start = 0;

        while start < characters.len() {
            let mut end = characters.len();
            let mut matched = None;

            while start < end {
                let candidate: String = characters[start..end].iter().collect();
                let candidate = if start == 0 {
                    candidate
                } else {
                    format!("##{candidate}")
                };

                if let Some(id) = self.vocabulary.get(&candidate) {
                    matched = Some((*id, end));
                    break;
                }

                end -= 1;
            }

            match matched {
                Some((id, next)) => {
                    pieces.push(id);
                    start = next;
                }
                None => {
                    // The whole word is unknown, not just this suffix: a partial
                    // tokenisation would be a different sentence.
                    return vec![self.unknown_id];
                }
            }
        }

        pieces
    }
}

/// Looks up a special token, or reports that the vocabulary cannot frame a
/// sequence.
fn special(vocabulary: &HashMap<String, i32>, token: &str) -> Result<i32, MlError> {
    vocabulary
        .get(token)
        .copied()
        .ok_or_else(|| MlError::Model {
            message: format!("the tokenizer vocabulary does not contain {token}"),
        })
}

/// Lower-cases, strips accents and removes control characters.
fn normalise(text: &str) -> String {
    let mut output = String::with_capacity(text.len());

    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_control() && !character.is_whitespace() {
            continue;
        }

        // A combining mark is the residue of a decomposed accent. Dropping it is
        // what turns `e` + U+0301 into `e`; the pre-composed letters are handled
        // by `without_accent` below.
        if ('\u{0300}'..='\u{036F}').contains(&character) {
            continue;
        }

        if character.is_whitespace() {
            output.push(' ');
        } else {
            output.push(without_accent(character));
        }
    }

    output
}

/// Maps the common accented Latin letters onto their unaccented form.
///
/// This is the documented subset: Latin-1 Supplement plus the Latin Extended-A
/// letters a European note actually contains. Anything else is returned
/// unchanged, reaches WordPiece as itself, and becomes `[UNK]` if the vocabulary
/// has no such piece.
fn without_accent(character: char) -> char {
    match character {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'ñ' | 'ń' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ő' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ś' | 'š' => 's',
        'ż' | 'ź' | 'ž' => 'z',
        'ř' => 'r',
        'ť' => 't',
        'ď' => 'd',
        'ğ' => 'g',
        // `ø`, `ß`, `ł`, `đ` and `þ` have no canonical decomposition, so
        // Unicode NFD — and therefore Hugging Face's strip-accents step — leaves
        // them alone. Mapping them here would disagree with the reference
        // tokenizer, so they fall through unchanged.
        _ => character,
    }
}

/// Splits normalised text on whitespace and punctuation, BERT's basic step.
fn basic_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for character in text.chars() {
        if character.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else if is_punctuation(character) {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(character.to_string());
        } else {
            current.push(character);
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Whether a character is punctuation for the basic split.
///
/// BERT treats any ASCII non-alphanumeric character as punctuation, and uses the
/// Unicode category for the rest. The crate has no `unicode-general-category`
/// access in `std`, so a non-ASCII, non-alphanumeric, non-whitespace character
/// is treated as punctuation. That is the same answer for every character in a
/// vocabulary this tokenizer is asked to serve.
fn is_punctuation(character: char) -> bool {
    if character.is_ascii() {
        character.is_ascii_punctuation()
    } else {
        !character.is_alphanumeric() && !character.is_whitespace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small vocabulary with the pieces the tests below need. Its ids are the
    /// line indexes, exactly like the real file, so an assertion on a number is
    /// also an assertion on the loader.
    fn fixture() -> WordPieceTokenizer {
        const VOCABULARY: &str = "[PAD]\n[UNK]\n[CLS]\n[SEP]\n[MASK]\nhello\nworld\nplay\n##ing\nun\n##believ\n##able\n,\nthe\nnote\n";
        WordPieceTokenizer::from_vocabulary(VOCABULARY).expect("the fixture vocabulary is valid")
    }

    #[test]
    fn the_loader_uses_the_line_index_as_the_id() {
        let tokenizer = fixture();

        assert_eq!(tokenizer.vocabulary_size(), 15);
        assert_eq!(tokenizer.unknown_id(), 1);
        assert_eq!(tokenizer.cls_id(), 2);
        assert_eq!(tokenizer.sep_id(), 3);
    }

    #[test]
    fn a_trailing_newline_is_not_a_token() {
        let with_newline = WordPieceTokenizer::from_vocabulary("[PAD]\n[UNK]\n[CLS]\n[SEP]\n")
            .expect("a trailing newline is normal");
        let without = WordPieceTokenizer::from_vocabulary("[PAD]\n[UNK]\n[CLS]\n[SEP]")
            .expect("a vocabulary without a trailing newline is valid");

        assert_eq!(with_newline.vocabulary_size(), without.vocabulary_size());
        assert_eq!(with_newline.vocabulary_size(), 4);
    }

    #[test]
    fn a_vocabulary_without_the_special_tokens_is_refused() {
        let error = WordPieceTokenizer::from_vocabulary("[PAD]\nhello\n")
            .expect_err("a vocabulary without [UNK], [CLS] and [SEP] cannot frame a sequence");

        assert!(matches!(error, MlError::Model { .. }));
    }

    #[test]
    fn a_known_word_is_one_piece() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("hello");

        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), 5, tokenizer.sep_id()],
            "hello is the fifth vocabulary line"
        );
    }

    #[test]
    fn a_word_needing_two_wordpieces_is_split() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("playing");

        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), 7, 8, tokenizer.sep_id()],
            "play + ##ing"
        );
    }

    #[test]
    fn a_word_needing_three_wordpieces_is_split_greedily() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("unbelievable");

        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), 9, 10, 11, tokenizer.sep_id()],
            "un + ##believ + ##able"
        );
    }

    #[test]
    fn an_unknown_word_is_the_unknown_token() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("kubernetes");

        assert_eq!(
            encoded.ids,
            vec![
                tokenizer.cls_id(),
                tokenizer.unknown_id(),
                tokenizer.sep_id()
            ]
        );
    }

    #[test]
    fn the_sequence_is_framed_with_cls_and_sep() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("hello world");

        assert_eq!(encoded.ids.first(), Some(&tokenizer.cls_id()));
        assert_eq!(encoded.ids.last(), Some(&tokenizer.sep_id()));
        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), 5, 6, tokenizer.sep_id()]
        );
    }

    #[test]
    fn the_attention_mask_has_one_element_per_id() {
        let tokenizer = fixture();

        for text in ["", "hello", "hello world, the note"] {
            let encoded = tokenizer.tokenize(text);
            assert_eq!(encoded.attention_mask.len(), encoded.ids.len());
            assert!(
                encoded.attention_mask.iter().all(|value| *value == 1.0),
                "one text is embedded per call, so nothing is padded"
            );
        }
    }

    #[test]
    fn an_empty_text_is_a_framed_empty_sequence() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("   ");

        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), tokenizer.sep_id()],
            "the encoder must never receive a zero-length sequence"
        );
    }

    #[test]
    fn the_text_is_lower_cased() {
        let tokenizer = fixture();

        assert_eq!(
            tokenizer.tokenize("HELLO").ids,
            tokenizer.tokenize("hello").ids
        );
    }

    #[test]
    fn an_accent_is_stripped_before_the_lookup() {
        // `café` normalises to `cafe`, which the fixture does not contain, so it
        // is unknown; what matters is that the accented form was not looked up
        // as itself. A vocabulary with `cafe` proves the strip positively.
        let tokenizer = WordPieceTokenizer::from_vocabulary("[PAD]\n[UNK]\n[CLS]\n[SEP]\ncafe\n")
            .expect("valid");

        assert_eq!(
            tokenizer.tokenize("café").ids,
            vec![tokenizer.cls_id(), 4, tokenizer.sep_id()]
        );
    }

    #[test]
    fn punctuation_is_its_own_token() {
        let tokenizer = fixture();
        let encoded = tokenizer.tokenize("hello, world");

        assert_eq!(
            encoded.ids,
            vec![tokenizer.cls_id(), 5, 12, 6, tokenizer.sep_id()]
        );
    }

    #[test]
    fn the_sequence_is_truncated_to_the_bound() {
        let tokenizer = fixture();
        let text = "hello ".repeat(MAX_TOKENS * 2);
        let encoded = tokenizer.tokenize(&text);

        assert_eq!(encoded.ids.len(), MAX_TOKENS);
        assert_eq!(encoded.ids.first(), Some(&tokenizer.cls_id()));
        assert_eq!(encoded.ids.last(), Some(&tokenizer.sep_id()));
        assert_eq!(encoded.attention_mask.len(), MAX_TOKENS);
    }

    #[test]
    fn a_word_over_the_character_bound_is_unknown() {
        let tokenizer = fixture();
        let word = "a".repeat(MAX_WORD_CHARS + 1);

        assert_eq!(
            tokenizer.tokenize(&word).ids,
            vec![
                tokenizer.cls_id(),
                tokenizer.unknown_id(),
                tokenizer.sep_id()
            ]
        );
    }
}
