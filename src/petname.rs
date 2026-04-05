//! Petname generator for session naming.
//!
//! Produces two-part names like `happy-tiger` or `calm-falcon` using
//! system time nanoseconds as a lightweight entropy source. No external
//! dependencies — just std.

const ADJECTIVES: &[&str] = &[
    "happy", "calm", "bright", "swift", "gentle", "bold", "warm", "cool", "keen", "wise", "brave",
    "clever", "eager", "fair", "glad", "kind", "lively", "merry", "noble", "proud", "quick",
    "sharp", "steady", "true", "vivid", "witty", "agile", "clear", "deft", "firm", "grand",
    "humble", "jolly", "light", "mild", "neat", "plain", "quiet", "ready", "sleek", "tidy", "able",
    "crisp", "free", "fresh", "lucid", "prime", "smooth", "stout", "zesty",
];

const ANIMALS: &[&str] = &[
    "tiger", "falcon", "otter", "panda", "robin", "dolphin", "fox", "owl", "lynx", "crane", "wolf",
    "hawk", "deer", "seal", "wren", "heron", "badger", "raven", "finch", "bison", "cobra", "eagle",
    "gecko", "horse", "ibis", "jay", "koala", "lemur", "moose", "newt", "ocelot", "pike", "quail",
    "rabbit", "salmon", "toucan", "urchin", "viper", "whale", "yak", "zebra", "bear", "crab",
    "dove", "elk", "frog", "goose", "hare", "iguana", "jackal",
];

/// Generate a two-part petname like `happy-tiger`.
///
/// Uses system time nanoseconds mixed with a simple hash to select
/// words. Not cryptographically random — just varied enough that
/// concurrent sessions are unlikely to collide.
pub fn generate() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    // Mix bits so nearby timestamps don't pick the same indices.
    // This is a simple xorshift-style mixing function.
    let mut h = nanos as u64;
    h ^= h >> 17;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 31;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 32;

    let adj_idx = (h as usize) % ADJECTIVES.len();
    // Use a different portion of the hash for the second word to
    // decorrelate the two selections.
    let animal_idx = ((h >> 16) as usize) % ANIMALS.len();

    format!("{}-{}", ADJECTIVES[adj_idx], ANIMALS[animal_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_returns_two_parts() {
        let name = generate();
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 2, "expected two-part name, got: {name}");
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn generate_uses_known_words() {
        let name = generate();
        let parts: Vec<&str> = name.split('-').collect();
        assert!(
            ADJECTIVES.contains(&parts[0]),
            "adjective '{}' not in word list",
            parts[0]
        );
        assert!(
            ANIMALS.contains(&parts[1]),
            "animal '{}' not in word list",
            parts[1]
        );
    }

    #[test]
    fn word_lists_have_expected_size() {
        assert!(ADJECTIVES.len() >= 50, "expected at least 50 adjectives");
        assert!(ANIMALS.len() >= 50, "expected at least 50 animals");
    }
}
