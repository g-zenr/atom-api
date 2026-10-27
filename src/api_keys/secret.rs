//! The API-key secret: its shape, its stored hash and its stored prefix.
//!
//! # The shape
//!
//! A secret is [`KEY_PREFIX`] followed by 32 bytes drawn from the operating
//! system's CSPRNG, rendered as lower-case hexadecimal:
//!
//! ```text
//! atom_4f3c…9ab1
//! ^^^^^ ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//! 5     64 hex characters (32 bytes)
//! ```
//!
//! That is **69 characters**, not 68: `atom_` is five characters, not four.
//! The byte count is the number that matters, not the character count — the
//! hashing argument below rests on 256 bits of entropy — so the length is
//! derived from [`SECRET_BYTES`] rather than written down in two places.
//!
//! The prefix is what the middleware switches on. A JWT is base64url text whose
//! first segment is a JSON header (`eyJ…`), so it can never begin with `atom_`:
//! the two credential kinds are distinguishable without trying to decode one as
//! the other.
//!
//! # Why SHA-256 and not Argon2
//!
//! The password rule in [`crate::auth::password`] is Argon2id, and that is
//! correct **for a password**: a password is drawn from a small, guessable
//! space, so the defence against an offline attacker is to make each guess
//! expensive.
//!
//! This secret is not that. It is 32 bytes of CSPRNG output — 256 bits — so
//! there is no dictionary to defend against, no guess an attacker can make more
//! cheaply than brute force, and no rainbow table that could ever be built. A
//! memory-hard hash would add nothing an attacker could exploit and everything
//! an operator could suffer: Argon2id at the configured parameters costs tens of
//! milliseconds and tens of megabytes, on **every authenticated request**, on
//! the async runtime. That is a self-inflicted denial of service.
//!
//! A single SHA-256 of the whole secret is therefore the right primitive: it is
//! fast, it is constant-time enough for a value an attacker cannot approach by
//! guessing, and it keeps the plaintext secret out of the database so a
//! database leak does not hand over working credentials.
//!
//! **This is deliberate and must not be "fixed" later.** Changing it to Argon2
//! would break every existing key and make each request slower for no security
//! gain. The reasoning is recorded in `docs/decisions/ADR-0011-api-keys.md`.

use rand::RngCore;
use sha2::{Digest, Sha256};

/// The prefix every API-key secret begins with.
///
/// The middleware switches on it, and `api_keys_prefix_chk` in the migration
/// pins the stored prefix to the same spelling.
pub const KEY_PREFIX: &str = "atom_";

/// Number of random bytes in the secret. 32 bytes is 256 bits.
const SECRET_BYTES: usize = 32;

/// Number of random bytes rendered as hex, i.e. the length of the random part.
const RANDOM_HEX_CHARS: usize = SECRET_BYTES * 2;

/// Number of hex characters of the random part kept in the stored `prefix`.
///
/// Eight characters identify a key in a listing without being enough to
/// reconstruct it: 32 bits of the secret, and the other 224 bits are absent.
/// `api_keys_prefix_chk` requires exactly `atom_` plus eight hex characters.
const PREFIX_TAIL_CHARS: usize = 8;

/// The length of a well-formed secret, in bytes: prefix plus random part.
const SECRET_CHARS: usize = KEY_PREFIX.len() + RANDOM_HEX_CHARS;

/// The lower-case hex digits, in value order.
const HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";

/// A freshly generated secret and the two values derived from it.
///
/// The three move together on purpose: generating a secret without its hash, or
/// storing a hash of something other than the secret handed to the client, is
/// the one mistake this type exists to make unrepresentable.
#[derive(Debug, Clone)]
pub struct GeneratedKey {
    /// The secret itself. This is the only copy that will ever exist.
    pub key: String,
    /// `atom_` plus the first eight hex characters of the random part.
    pub prefix: String,
    /// SHA-256 of the whole secret, lower-case hex. This is what is stored.
    pub key_hash: String,
}

/// Whether a bearer token is an API key rather than a JWT.
///
/// The check is the prefix alone, as the contract specifies: it is a routing
/// decision, not a validation. A token that starts with `atom_` but is
/// otherwise nonsense is routed here and then refused by
/// [`crate::api_keys::service::ApiKeyService::authenticate`], which is what
/// makes a malformed key a `401` rather than a decode error.
pub fn is_api_key(token: &str) -> bool {
    token.starts_with(KEY_PREFIX)
}

/// Whether a string has the shape of a generated secret.
///
/// A value that fails this check cannot be the hash of any row this module
/// wrote, so it is refused before the hash is computed and the statement runs.
/// That keeps a garbage `atom_…` token a `401` and stops an arbitrarily long
/// string from reaching the hash function.
///
/// The random part is **lower-case** hex only, because that is how
/// [`generate`] renders it; an upper-case variant would hash to a different
/// value and could never match a stored hash anyway.
pub fn is_well_formed(secret: &str) -> bool {
    secret.len() == SECRET_CHARS
        && secret.starts_with(KEY_PREFIX)
        && secret[KEY_PREFIX.len()..]
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Generates a secret from the operating system's CSPRNG.
///
/// `rand::rng()` is the thread-local generator seeded from the OS entropy
/// source; it is the same source [`crate::auth::token`] uses for refresh
/// tokens. No `SeedableRng` with a fixed seed is used anywhere in this path.
pub fn generate() -> GeneratedKey {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rng().fill_bytes(&mut bytes);

    let random = hex_lower(&bytes);
    let key = format!("{KEY_PREFIX}{random}");
    let prefix = format!("{KEY_PREFIX}{}", &random[..PREFIX_TAIL_CHARS]);

    GeneratedKey {
        key_hash: hash(&key),
        key,
        prefix,
    }
}

/// Computes the stored hash of a secret: SHA-256, lower-case hex.
///
/// The **whole** secret is hashed, prefix included, so two keys that differ
/// only in their random part never collide on a shared prefix. The rationale
/// for SHA-256 rather than Argon2 is in the module documentation.
pub fn hash(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());

    // `{:x}` renders the digest as lower-case hexadecimal without separators,
    // which is what `api_keys_key_hash_chk` requires and what the lookup binds.
    format!("{:x}", hasher.finalize())
}

/// Renders bytes as lower-case hex.
///
/// A local function rather than a `hex` crate dependency: the crate already
/// depends on `sha2`, whose formatter covers the digest, and this is the only
/// other place in the module that needs an encoding.
fn hex_lower(bytes: &[u8]) -> String {
    let mut rendered = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        // A nibble is 0..=15, so the index is always in range; `char::from`
        // needs no failure path.
        rendered.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        rendered.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_key_has_the_published_shape() {
        let generated = generate();

        assert!(
            generated.key.starts_with("atom_"),
            "every secret starts with the routing prefix"
        );
        assert_eq!(
            generated.key.len(),
            5 + 64,
            "atom_ plus 32 bytes rendered as hex is 69 characters"
        );
        assert!(is_well_formed(&generated.key));

        // The prefix is the routing prefix plus the first eight hex characters,
        // so it is a genuine prefix of the secret and not an arbitrary token.
        assert_eq!(generated.prefix.len(), 5 + 8);
        assert_eq!(&generated.key[..generated.prefix.len()], generated.prefix);

        // The stored hash is SHA-256 of the whole secret, lower-case hex.
        assert_eq!(generated.key_hash.len(), 64);
        assert_eq!(generated.key_hash, hash(&generated.key));
        assert!(
            generated
                .key_hash
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
            "the stored hash is lower-case hex"
        );
    }

    #[test]
    fn two_generated_keys_differ() {
        let first = generate();
        let second = generate();

        assert_ne!(first.key, second.key);
        assert_ne!(first.key_hash, second.key_hash);
    }

    #[test]
    fn the_hash_is_the_published_sha256_of_the_whole_secret() {
        // A known value pins the primitive and the encoding: changing either
        // would silently invalidate every stored key.
        assert_eq!(
            hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_token_is_an_api_key_exactly_when_it_carries_the_prefix() {
        assert!(is_api_key("atom_deadbeef"));
        assert!(is_api_key(&generate().key));

        // A JWT's first segment is a base64url-encoded JSON header.
        assert!(!is_api_key(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.signature"
        ));
        assert!(!is_api_key(""));
        assert!(!is_api_key("Bearer atom_deadbeef"));
    }

    #[test]
    fn only_a_correctly_shaped_secret_is_well_formed() {
        assert!(is_well_formed(&generate().key));

        let valid = generate().key;
        let cases = [
            // No prefix.
            valid.trim_start_matches("atom_").to_string(),
            // Too short, too long.
            "atom_0123456789abcdef".to_string(),
            format!("{valid}0"),
            // Upper-case hex cannot be produced by `generate`.
            format!("atom_{}", "A".repeat(64)),
            // A non-hex character in the random part.
            format!("atom_{}", "g".repeat(64)),
            // Whitespace is not trimmed by the middleware, and must not be by
            // this check either.
            format!("{valid} "),
            String::new(),
        ];

        for case in cases {
            assert!(
                !is_well_formed(&case),
                "{case:?} is not a generated secret and must be refused"
            );
        }

        assert!(is_well_formed(&format!("atom_{}", "0".repeat(64))));
    }
}
