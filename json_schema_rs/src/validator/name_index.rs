//! Names laid out for lookup: the names and their heads in parallel arrays, and an open-addressed
//! index over the heads. A `properties` list is one, with its schemas alongside; an `enum` list
//! keeps one of its strings.
//!
//! The evaluator looks up every member of an object it checks against `properties` and
//! `additionalProperties`, and every string it checks against an `enum`. A binary search over the sorted names costs a call to `memcmp` and a
//! mispredicted branch per step; the index costs one multiply and, nearly always, one probe whose
//! head comparison decides without reading the name. A short list skips the index: its heads are
//! compared in turn.

use super::compiled::NodeId;

/// The first and last eight bytes of a name and its length: equal heads mean equal names for
/// names of up to sixteen bytes, and longer names compare only the bytes in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Head {
    len: usize,
    first: u64,
    last: u64,
}

/// The largest name whose head holds every byte.
const INLINE_BYTES: usize = 16;

/// The most names an index compares one by one instead of indexing: a scan over a few heads is
/// cheaper than hashing.
const LINEAR_NAMES: usize = 4;

impl Head {
    /// The head of no name: no name is `usize::MAX` bytes long.
    const NONE: Head = Head {
        len: usize::MAX,
        first: 0,
        last: 0,
    };

    #[inline]
    fn of(name: &[u8]) -> Head {
        let len: usize = name.len();
        let (first, last): (u64, u64) = if let (Some(first), Some(last)) =
            (name.first_chunk::<8>(), name.last_chunk::<8>())
        {
            (u64::from_le_bytes(*first), u64::from_le_bytes(*last))
        } else if let (Some(first), Some(last)) = (name.first_chunk::<4>(), name.last_chunk::<4>())
        {
            (
                u64::from(u32::from_le_bytes(*first)),
                u64::from(u32::from_le_bytes(*last)),
            )
        } else {
            // A byte loop, not a copy: a copy of variable length is a call to `memcpy`.
            let mut packed: u64 = 0;
            for (i, byte) in name.iter().enumerate() {
                packed |= u64::from(*byte) << (8 * i);
            }
            (packed, 0)
        };
        Head { len, first, last }
    }

    /// The slot this head starts probing at, among `1 << (64 - shift)` slots.
    #[inline]
    fn slot(self, shift: u32) -> usize {
        let mixed: u64 = (self.first ^ self.last.rotate_left(29) ^ self.len as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "shifted below the slot count, which is a usize"
        )]
        let slot: usize = (mixed >> shift) as usize;
        slot
    }
}

/// An index over names, each found by its position (a name given twice, at its first). It keeps only the names' heads:
/// the caller owns the names and gives the index the name at a position when a long name needs
/// its middle bytes compared, so an index adds no copy of its names.
#[derive(Debug, PartialEq)]
pub(crate) struct NameIndex {
    heads: Box<[Head]>,
    /// Open addressing with linear probing: a name's position plus one, `0` for an empty slot.
    /// A power of two, at least twice the names, so a probe ends at an empty slot soon. Empty for
    /// at most [`LINEAR_NAMES`] names, whose heads are scanned instead.
    slots: Box<[u32]>,
    /// `64 - log2(slots.len())`: the hash bits that pick a slot.
    shift: u32,
}

impl NameIndex {
    /// The index of `names`; a `None` takes a position no name is found at (an `enum` value that
    /// is not a string). Both lookups reach a name's first position before any later one: the
    /// scan goes in order, and a later copy is inserted after it, further along the same probe.
    pub(crate) fn new<'n>(names: impl Iterator<Item = Option<&'n str>>) -> NameIndex {
        let heads: Box<[Head]> = names
            .map(|name| name.map_or(Head::NONE, |name| Head::of(name.as_bytes())))
            .collect();
        if heads.len() <= LINEAR_NAMES {
            return NameIndex {
                heads,
                slots: Box::new([]),
                shift: 0,
            };
        }
        let count: usize = (2 * heads.len()).next_power_of_two();
        let shift: u32 = 64 - count.trailing_zeros();
        let mut slots: Box<[u32]> = vec![0; count].into_boxed_slice();
        for (position, head) in heads.iter().enumerate() {
            if *head == Head::NONE {
                continue;
            }
            let mut slot: usize = head.slot(shift);
            while slots[slot] != 0 {
                slot = (slot + 1) & (count - 1);
            }
            slots[slot] = u32::try_from(position + 1).unwrap_or(u32::MAX);
        }
        NameIndex {
            heads,
            slots,
            shift,
        }
    }

    /// The position of `name`, if the index has it; `name_at` gives the indexed name at a
    /// position.
    #[inline]
    pub(crate) fn find<'n>(&self, name: &str, name_at: impl Fn(usize) -> &'n str) -> Option<usize> {
        let bytes: &[u8] = name.as_bytes();
        let head: Head = Head::of(bytes);
        let matches = |position: usize| {
            self.heads[position] == head
                && (head.len <= INLINE_BYTES
                    || name_at(position).as_bytes()[8..head.len - 8] == bytes[8..head.len - 8])
        };
        if self.slots.is_empty() {
            return (0..self.heads.len()).find(|position| matches(*position));
        }
        let mask: usize = self.slots.len() - 1;
        let mut slot: usize = head.slot(self.shift);
        loop {
            let entry: u32 = self.slots[slot];
            let position: usize = entry.checked_sub(1)? as usize;
            if matches(position) {
                return Some(position);
            }
            slot = (slot + 1) & mask;
        }
    }
}

/// A `properties` list: each name and its schema, sorted by name, with an index for lookup by
/// name.
#[derive(Debug, PartialEq)]
pub(crate) struct PropertyTable {
    names: Box<[Box<str>]>,
    nodes: Box<[NodeId]>,
    index: NameIndex,
}

impl PropertyTable {
    /// The table of `properties`, which must be sorted by name and free of duplicates.
    pub(crate) fn new(properties: Vec<(Box<str>, NodeId)>) -> PropertyTable {
        debug_assert!(properties.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let (names, nodes): (Vec<Box<str>>, Vec<NodeId>) = properties.into_iter().unzip();
        PropertyTable {
            index: NameIndex::new(names.iter().map(|name| Some(&**name))),
            names: names.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
        }
    }

    #[inline]
    fn find(&self, name: &str) -> Option<usize> {
        self.index.find(name, |position| &self.names[position])
    }

    /// Whether the table lists `name`.
    #[inline]
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    /// The schema of `name`, if the table lists it.
    #[inline]
    pub(crate) fn get(&self, name: &str) -> Option<NodeId> {
        self.find(name).map(|position| self.nodes[position])
    }

    /// How many names the table lists.
    pub(crate) fn len(&self) -> usize {
        self.names.len()
    }

    /// Each name and its schema, sorted by name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, NodeId)> {
        self.names
            .iter()
            .map(|name| &**name)
            .zip(self.nodes.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::{NameIndex, NodeId, PropertyTable};

    fn table(names: &[&str]) -> PropertyTable {
        let mut properties: Vec<(Box<str>, NodeId)> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (Box::from(*name), NodeId::from_index(i)))
            .collect();
        properties.sort_by(|a, b| a.0.cmp(&b.0));
        PropertyTable::new(properties)
    }

    /// Every lookup of `keys` in the table of `names`: the name found, if any.
    fn lookups<'t>(names: &[&str], keys: &[&'t str]) -> Vec<Option<&'t str>> {
        let table: PropertyTable = table(names);
        keys.iter()
            .map(|key| table.contains(key).then_some(*key))
            .collect()
    }

    #[test]
    fn find_tells_names_of_every_length_apart() {
        let names: [&str; 9] = [
            "",
            "a",
            "ab",
            "abcd",
            "abcdefg",
            "abcdefgh",
            "abcdefghijklmnop",
            "abcdefghijklmnopq",
            "abcdefghij_lmnopqrstuvwxyz",
        ];
        let keys: [&str; 14] = [
            "",
            "a",
            "ab",
            "abcd",
            "abcdefg",
            "abcdefgh",
            "abcdefghijklmnop",
            "abcdefghijklmnopq",
            "abcdefghij_lmnopqrstuvwxyz",
            "abc",
            "a\0",
            "abcdefgh\0",
            "abcdefghijklmnoq",
            "abcdefghijKlmnopqrstuvwxyz",
        ];
        let expected: Vec<Option<&str>> = vec![
            Some(""),
            Some("a"),
            Some("ab"),
            Some("abcd"),
            Some("abcdefg"),
            Some("abcdefgh"),
            Some("abcdefghijklmnop"),
            Some("abcdefghijklmnopq"),
            Some("abcdefghij_lmnopqrstuvwxyz"),
            None,
            None,
            None,
            None,
            None,
        ];
        let actual: Vec<Option<&str>> = lookups(&names, &keys);
        assert_eq!(expected, actual);
    }

    #[test]
    fn get_gives_each_name_its_own_schema() {
        let names: Vec<String> = (0..100).map(|i| format!("property{i}")).collect();
        let table: PropertyTable = table(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let expected: Vec<Option<NodeId>> = (0..100).map(|i| Some(NodeId::from_index(i))).collect();
        let actual: Vec<Option<NodeId>> = names.iter().map(|name| table.get(name)).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn find_in_a_short_list_tells_names_apart() {
        let names: [&str; 3] = ["id", "abcdefghijklmnopq", "name"];
        let keys: [&str; 5] = [
            "name",
            "id",
            "abcdefghijklmnopq",
            "abcdefghijklmnoPq",
            "nam",
        ];
        let expected: Vec<Option<&str>> = vec![
            Some("name"),
            Some("id"),
            Some("abcdefghijklmnopq"),
            None,
            None,
        ];
        let actual: Vec<Option<&str>> = lookups(&names, &keys);
        assert_eq!(expected, actual);
    }

    #[test]
    fn index_finds_each_name_at_its_given_position() {
        let names: Vec<String> = (0..20).rev().map(|i| format!("team_{i}")).collect();
        let index: NameIndex = NameIndex::new(names.iter().map(|name| Some(name.as_str())));
        let expected: Vec<Option<usize>> = (0..20).map(Some).chain([None]).collect();
        let actual: Vec<Option<usize>> = names
            .iter()
            .map(String::as_str)
            .chain(["team_20"])
            .map(|name| index.find(name, |position| &names[position]))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn find_in_an_empty_table_finds_nothing() {
        let expected: Vec<Option<&str>> = vec![None];
        let actual: Vec<Option<&str>> = lookups(&[], &["a"]);
        assert_eq!(expected, actual);
    }
}
