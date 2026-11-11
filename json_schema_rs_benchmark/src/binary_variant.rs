//! Executable variants for the quick mode's CLI tier (#123).
//!
//! On macOS, a process's peak RSS depends on the identity of the executable it runs, not only
//! on its code: copies of one `jsonschemars` build that differ only in their Mach-O UUID (and so
//! in their code signature) peaked anywhere from 21.6 to 22.4 MiB on `generated/many-defs`
//! codegen, each copy consistently, and a copy differing in one byte of header padding moved as
//! much. Two builds of the same commit (the base and head of an A/A run) are two such identities,
//! so their RSS differed by 3.2–3.4% in every round, and the quick mode reported it as a move.
//! In-process time showed no such effect.
//!
//! The quick mode therefore runs each CLI round on its own variant of each side's executable
//! ([`rewrite_mach_o_uuid`], then re-signed), so the effect of a build's identity varies from
//! round to round and lands in the spread, like the per-process layout effect of #100.

/// The Mach-O 64-bit magic number, as stored little-endian (`MH_MAGIC_64`).
const MH_MAGIC_64: u32 = 0xfeed_facf;
/// Size of the 64-bit Mach-O header, where the load commands start.
const HEADER_SIZE: usize = 32;
/// Offset of `ncmds` in the 64-bit Mach-O header.
const NCMDS_OFFSET: usize = 16;
/// The `LC_UUID` load command.
const LC_UUID: u32 = 0x1b;
/// Offset of the 16 UUID bytes in an `LC_UUID` command (after `cmd` and `cmdsize`).
const UUID_OFFSET: usize = 8;

/// Why an executable could not be rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantError {
    /// The file is not a little-endian 64-bit Mach-O executable (on Linux, an ELF file).
    NotMachO,
    /// The load commands are cut short or have no `LC_UUID`.
    NoUuid,
}

impl std::fmt::Display for VariantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotMachO => f.write_str("not a 64-bit little-endian Mach-O executable"),
            Self::NoUuid => f.write_str("no LC_UUID load command"),
        }
    }
}

impl std::error::Error for VariantError {}

/// The `u32` at `offset`, little-endian; `None` past the end.
fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let field: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(field))
}

/// The UUID of variant `seed`: two rounds of splitmix64, so neighbouring seeds share no bytes.
#[must_use]
pub fn variant_uuid(seed: u64) -> [u8; 16] {
    let mix = |value: u64| -> u64 {
        let mut z: u64 = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let low: u64 = mix(seed);
    let high: u64 = mix(low);
    let mut uuid: [u8; 16] = [0; 16];
    uuid[..8].copy_from_slice(&low.to_le_bytes());
    uuid[8..].copy_from_slice(&high.to_le_bytes());
    uuid
}

/// Replace the UUID of a 64-bit Mach-O executable with [`variant_uuid`]`(seed)`. The code
/// signature no longer matches afterwards: re-sign the result (`codesign -f -s -`).
///
/// # Errors
///
/// [`VariantError::NotMachO`] when `bytes` is not a little-endian 64-bit Mach-O file, and
/// [`VariantError::NoUuid`] when its load commands have no `LC_UUID`.
pub fn rewrite_mach_o_uuid(bytes: &mut [u8], seed: u64) -> Result<(), VariantError> {
    if read_u32(bytes, 0) != Some(MH_MAGIC_64) {
        return Err(VariantError::NotMachO);
    }
    let commands: u32 = read_u32(bytes, NCMDS_OFFSET).ok_or(VariantError::NotMachO)?;
    let mut offset: usize = HEADER_SIZE;
    for _ in 0..commands {
        let command: u32 = read_u32(bytes, offset).ok_or(VariantError::NoUuid)?;
        let size: u32 = read_u32(bytes, offset + 4).ok_or(VariantError::NoUuid)?;
        if command == LC_UUID {
            let start: usize = offset + UUID_OFFSET;
            let uuid: &mut [u8] = bytes
                .get_mut(start..start + 16)
                .ok_or(VariantError::NoUuid)?;
            uuid.copy_from_slice(&variant_uuid(seed));
            return Ok(());
        }
        let size: usize = usize::try_from(size).map_err(|_| VariantError::NoUuid)?;
        if size == 0 {
            return Err(VariantError::NoUuid);
        }
        offset = offset.checked_add(size).ok_or(VariantError::NoUuid)?;
    }
    Err(VariantError::NoUuid)
}

#[cfg(test)]
mod tests {
    use super::{VariantError, rewrite_mach_o_uuid, variant_uuid};

    /// A Mach-O header with two load commands: a 16-byte one, then `LC_UUID` holding `0xaa`s.
    fn mach_o() -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend(0xfeed_facf_u32.to_le_bytes());
        bytes.extend([0; 12]);
        bytes.extend(2_u32.to_le_bytes());
        bytes.extend([0; 12]);
        bytes.extend(0x19_u32.to_le_bytes());
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend(0x1b_u32.to_le_bytes());
        bytes.extend(24_u32.to_le_bytes());
        bytes.extend([0xaa; 16]);
        bytes
    }

    #[test]
    fn rewrite_mach_o_uuid_replaces_only_the_uuid() {
        let mut expected: Vec<u8> = mach_o();
        expected[56..72].copy_from_slice(&variant_uuid(3));
        let mut actual: Vec<u8> = mach_o();
        let _ = rewrite_mach_o_uuid(&mut actual, 3);
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_mach_o_uuid_rejects_another_format() {
        let mut bytes: Vec<u8> = b"\x7fELF and the rest".to_vec();
        let expected: Result<(), VariantError> = Err(VariantError::NotMachO);
        let actual: Result<(), VariantError> = rewrite_mach_o_uuid(&mut bytes, 1);
        assert_eq!(expected, actual);
    }

    #[test]
    fn rewrite_mach_o_uuid_without_uuid_command_fails() {
        let mut bytes: Vec<u8> = mach_o();
        bytes.truncate(48);
        bytes[16..20].copy_from_slice(&1_u32.to_le_bytes());
        let expected: Result<(), VariantError> = Err(VariantError::NoUuid);
        let actual: Result<(), VariantError> = rewrite_mach_o_uuid(&mut bytes, 1);
        assert_eq!(expected, actual);
    }

    #[test]
    fn variant_uuids_differ_between_seeds() {
        let expected: usize = 16;
        let actual: usize = (0..16_u64)
            .map(variant_uuid)
            .collect::<std::collections::BTreeSet<[u8; 16]>>()
            .len();
        assert_eq!(expected, actual);
    }
}
