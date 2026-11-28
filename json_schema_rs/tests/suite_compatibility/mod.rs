//! Which drafts a JSON Schema Test Suite case applies to: its `compatibility` property.
//!
//! Shared by every reader of the suite (the validation and annotation suite gates, codegen's
//! merge check, the differential report), each including this file with `#[path]` or `mod`. Its unit tests live in the differential
//! crate (`json_schema_rs_differential/src/suite.rs`): the suite gates run without the test harness.
//! The suite numbers drafts by release: `4`, `6`, `7`, `2019` (2019-09) and `2020` (2020-12).

/// Whether `compatibility` admits `release`, by the suite's own algorithm
/// (`validation/README.md`, *Compatibility matching*): constraints are comma-separated `N`
/// (`N` and later), `<=N` (`N` and earlier) or `=N` (`N` only), ordered smallest to largest, and
/// the first constraint whose bound is at or above `release` decides. No `compatibility` admits
/// every release.
///
/// # Errors
///
/// A constraint whose bound is not a number.
pub fn compatible(compatibility: Option<&str>, release: u32) -> Result<bool, String> {
    let Some(compatibility) = compatibility else {
        return Ok(true);
    };
    let mut admits: bool = true;
    for constraint in compatibility.split(',').map(str::trim) {
        let (number, test): (&str, fn(u32, u32) -> bool) =
            if let Some(number) = constraint.strip_prefix("<=") {
                (number, |release, bound| release <= bound)
            } else if let Some(number) = constraint.strip_prefix('=') {
                (number, |release, bound| release == bound)
            } else {
                (constraint, |release, bound| release >= bound)
            };
        let bound: u32 = number
            .parse()
            .map_err(|e| format!("bad compatibility {compatibility:?}: {e}"))?;
        admits = test(release, bound);
        if release <= bound {
            break;
        }
    }
    Ok(admits)
}
