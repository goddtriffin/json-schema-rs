//! Known-failing regression tests for `#[derive(ToJsonSchema)]`: one or more tests per open
//! bug issue, named `issue_<N>_...`.
//!
//! Each test body asserts the **correct** behaviour. The wrapper inverts it: the test passes
//! while the body still fails, and fails as soon as the body passes ("unexpected pass"). When
//! an issue is fixed, unwrap its tests into plain `#[test]`s. [`known_failure`] mirrors the
//! helper in `json_schema_rs/tests/known_failures.rs` (a separate crate, so it cannot be
//! shared); a bug that aborts the process needs that file's `known_failure_in_child_process`.

use std::panic::{AssertUnwindSafe, catch_unwind};

/// What happened when a known-failing test body ran.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The body completed: the bug is fixed.
    Passed,
    /// The body panicked: the bug is still present.
    Failed,
}

fn unexpected_pass_message(issue: u32) -> String {
    format!(
        "#{issue} now behaves correctly: remove the known-failure wrapper so this runs as a \
         normal test (see design.md, Known failures)"
    )
}

/// Runs `body`, which asserts the correct behaviour for `issue`; passes while `body` panics.
#[expect(
    dead_code,
    reason = "no open derive bug has a known-failing test; remove this when adding one"
)]
fn known_failure(issue: u32, body: impl FnOnce()) {
    let expected: Outcome = Outcome::Failed;
    let actual: Outcome = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(()) => Outcome::Passed,
        Err(_) => Outcome::Failed,
    };
    assert_eq!(expected, actual, "{}", unexpected_pass_message(issue));
}
