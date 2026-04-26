//! End-to-end CLI smoke test for the B14 agent taxonomy surface.
//!
//! Exercises the user-facing flag and subcommand surface without
//! invoking `claude`, so the test is hermetic and runs in CI without
//! network or auth.
//!
//! Coverage:
//! 1. `--help` lists `--persona`, `--role`, `--identity`, `--theme` —
//!    proves the four taxonomy primitives are wired into the CLI.
//! 2. `persona list` lists embedded themes — proves the embedded
//!    roster is loadable.
//! 3. `persona list <theme>` lists characters in a known theme —
//!    proves theme-roster traversal end to end.
//! 4. `persona show <theme> --agent <character>` prints the resolved
//!    character card — proves theme + persona fuzzy resolution + the
//!    Character lookup succeed end to end.
//! 5. `persona show` accepts a fuzzy theme fragment — proves the
//!    fuzzy resolver is wired to the subcommand path.

use std::process::Command;

/// Path to the `forestage` binary built by Cargo for this test run.
fn forestage_bin() -> &'static str {
    env!("CARGO_BIN_EXE_forestage")
}

/// Run forestage with the given args and return (stdout, stderr,
/// exit-success). Test helper.
fn run(args: &[&str]) -> (String, String, bool) {
    let output = Command::new(forestage_bin())
        .args(args)
        .output()
        .expect("forestage binary runs");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.success(),
    )
}

#[test]
fn help_advertises_taxonomy_flags() {
    let (stdout, _stderr, ok) = run(&["--help"]);
    assert!(ok, "forestage --help must succeed");
    for flag in ["--theme", "--persona", "--role", "--identity"] {
        assert!(
            stdout.contains(flag),
            "--help output must advertise {flag} (B14 taxonomy flag), \
             got:\n{stdout}"
        );
    }
}

#[test]
fn persona_list_emits_known_themes() {
    let (stdout, _stderr, ok) = run(&["persona", "list"]);
    assert!(ok, "persona list must succeed");
    // discworld and the-expanse are both shipped in the embedded set
    // and used elsewhere in the test surface; pin to them explicitly.
    for slug in ["discworld", "the-expanse"] {
        assert!(
            stdout.contains(slug),
            "persona list must include {slug}, got:\n{stdout}"
        );
    }
}

#[test]
fn persona_list_theme_emits_characters() {
    let (stdout, _stderr, ok) = run(&["persona", "list", "discworld"]);
    assert!(ok, "persona list discworld must succeed");
    // Granny + Ponder are the regression fixtures from finding-033;
    // both must appear in the discworld roster.
    for name in ["Granny", "Ponder"] {
        assert!(
            stdout.contains(name),
            "discworld roster must include {name}, got:\n{stdout}"
        );
    }
}

#[test]
fn persona_show_resolves_character_by_slug() {
    let (stdout, _stderr, ok) = run(&[
        "persona",
        "show",
        "discworld",
        "--agent",
        "granny-weatherwax",
    ]);
    assert!(
        ok,
        "persona show discworld --agent granny-weatherwax must succeed"
    );
    assert!(
        stdout.contains("Granny Weatherwax"),
        "expected character card for Granny, got:\n{stdout}"
    );
}

#[test]
fn persona_show_accepts_fuzzy_theme_fragment() {
    // "disc" is a unique prefix for "discworld" — fuzzy resolver
    // should pick it without prompting.
    let (stdout, _stderr, ok) = run(&["persona", "show", "disc"]);
    assert!(ok, "persona show disc must succeed via fuzzy resolution");
    assert!(
        stdout.to_lowercase().contains("discworld") || stdout.contains("Discworld"),
        "expected Discworld theme details, got:\n{stdout}"
    );
}
