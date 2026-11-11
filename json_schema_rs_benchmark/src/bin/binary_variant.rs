//! Writes a variant of an executable whose identity differs (#123; see
//! `json_schema_rs_benchmark::binary_variant`): `binary_variant <input> <output> <seed>`.
//!
//! A Mach-O executable gets the UUID of `seed`; re-sign the output (`codesign -f -s -`) before
//! running it. Exits 2, writing nothing, when the input is not a Mach-O executable (on Linux),
//! so the caller can fall back to a plain copy.

use json_schema_rs_benchmark::binary_variant::{VariantError, rewrite_mach_o_uuid};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, seed] = args.as_slice() else {
        eprintln!("usage: binary_variant <input> <output> <seed>");
        return ExitCode::FAILURE;
    };
    let Ok(seed) = seed.parse::<u64>() else {
        eprintln!("binary_variant: seed `{seed}` is not a non-negative integer");
        return ExitCode::FAILURE;
    };
    let mut bytes: Vec<u8> = match std::fs::read(input) {
        Ok(bytes) => bytes,
        Err(err) => {
            eprintln!("binary_variant: reading {input}: {err}");
            return ExitCode::FAILURE;
        }
    };
    match rewrite_mach_o_uuid(&mut bytes, seed) {
        Ok(()) => {}
        Err(VariantError::NotMachO) => {
            eprintln!("binary_variant: {input}: {}", VariantError::NotMachO);
            return ExitCode::from(2);
        }
        Err(err) => {
            eprintln!("binary_variant: {input}: {err}");
            return ExitCode::FAILURE;
        }
    }
    if let Err(err) = std::fs::write(output, &bytes) {
        eprintln!("binary_variant: writing {output}: {err}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
