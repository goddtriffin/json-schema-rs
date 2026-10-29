//! The `type` keyword: the seven JSON Schema primitive types and a set of them.

use super::dialect::IntegerSemantics;
use crate::number;
use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::str::FromStr;

/// One of the seven JSON Schema primitive types (the `simpleTypes` of every draft's metaschema).
/// Variants are in name order, which is the order a [`TypeSet`] lists them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JsonType {
    /// `"array"`.
    Array,
    /// `"boolean"`.
    Boolean,
    /// `"integer"`: a number that is an integer under the dialect's
    /// [`IntegerSemantics`].
    Integer,
    /// `"null"`.
    Null,
    /// `"number"`: any number, integers included.
    Number,
    /// `"object"`.
    Object,
    /// `"string"`.
    String,
}

impl JsonType {
    /// Every type, in name order.
    pub const ALL: [JsonType; 7] = [
        JsonType::Array,
        JsonType::Boolean,
        JsonType::Integer,
        JsonType::Null,
        JsonType::Number,
        JsonType::Object,
        JsonType::String,
    ];

    /// The type's name as the `type` keyword spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            JsonType::Array => "array",
            JsonType::Boolean => "boolean",
            JsonType::Integer => "integer",
            JsonType::Null => "null",
            JsonType::Number => "number",
            JsonType::Object => "object",
            JsonType::String => "string",
        }
    }

    /// This type's bit in a [`TypeSet`].
    const fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

impl fmt::Display for JsonType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A name that is not one of the seven JSON Schema primitive types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownJsonType(pub String);

impl fmt::Display for UnknownJsonType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown type \"{}\", expected one of \"array\", \"boolean\", \"integer\", \"null\", \
             \"number\", \"object\", \"string\"",
            self.0
        )
    }
}

impl std::error::Error for UnknownJsonType {}

impl FromStr for JsonType {
    type Err = UnknownJsonType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        JsonType::ALL
            .into_iter()
            .find(|t| t.as_str() == s)
            .ok_or_else(|| UnknownJsonType(s.to_string()))
    }
}

/// The value of the `type` keyword: a set of [`JsonType`]s, held as a bitset.
///
/// An instance matches when its type is in the set; a number with an integer value also matches
/// [`JsonType::Integer`] (see [`TypeSet::matches`]). The keyword's string form is a set of one;
/// its array form keeps every entry (duplicates collapse). The empty set (`"type": []`, which the
/// metaschemas disallow but whose meaning is clear) matches nothing.
///
/// Serializes as a string when it holds one type and as an array in name order otherwise, so a
/// round trip normalises the written order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeSet(u8);

impl TypeSet {
    /// The set with no types.
    #[must_use]
    pub const fn empty() -> Self {
        TypeSet(0)
    }

    /// Whether `ty` is in the set.
    #[must_use]
    pub const fn contains(self, ty: JsonType) -> bool {
        self.0 & ty.bit() != 0
    }

    /// Adds `ty` to the set.
    pub const fn insert(&mut self, ty: JsonType) {
        self.0 |= ty.bit();
    }

    /// The set with `ty` added.
    #[must_use]
    pub const fn with(mut self, ty: JsonType) -> Self {
        self.insert(ty);
        self
    }

    /// The set with `ty` removed.
    #[must_use]
    pub const fn without(self, ty: JsonType) -> Self {
        TypeSet(self.0 & !ty.bit())
    }

    /// Whether the set has no types.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The number of types in the set.
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The set's only type, when it holds exactly one.
    #[must_use]
    pub fn single(self) -> Option<JsonType> {
        let mut types = self.iter();
        match (types.next(), types.next()) {
            (Some(ty), None) => Some(ty),
            _ => None,
        }
    }

    /// The set's types in name order.
    pub fn iter(self) -> impl Iterator<Item = JsonType> {
        JsonType::ALL.into_iter().filter(move |t| self.contains(*t))
    }

    /// Whether `instance` matches the set: its JSON type is in the set, or it is a number that
    /// is an integer under `integer_semantics` and the set holds [`JsonType::Integer`].
    #[must_use]
    pub fn matches(self, instance: &Value, integer_semantics: IntegerSemantics) -> bool {
        match instance {
            Value::Null => self.contains(JsonType::Null),
            Value::Bool(_) => self.contains(JsonType::Boolean),
            Value::Number(n) => {
                self.contains(JsonType::Number)
                    || (self.contains(JsonType::Integer)
                        && number::is_integer(n, integer_semantics))
            }
            Value::String(_) => self.contains(JsonType::String),
            Value::Array(_) => self.contains(JsonType::Array),
            Value::Object(_) => self.contains(JsonType::Object),
        }
    }
}

impl From<JsonType> for TypeSet {
    fn from(ty: JsonType) -> Self {
        TypeSet(ty.bit())
    }
}

impl FromIterator<JsonType> for TypeSet {
    fn from_iter<I: IntoIterator<Item = JsonType>>(iter: I) -> Self {
        let mut set: TypeSet = TypeSet::empty();
        for ty in iter {
            set.insert(ty);
        }
        set
    }
}

impl fmt::Display for TypeSet {
    /// One type as its name (`string`); several as a bracketed list (`[integer, string]`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ty) = self.single() {
            return f.write_str(ty.as_str());
        }
        f.write_str("[")?;
        for (index, ty) in self.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            f.write_str(ty.as_str())?;
        }
        f.write_str("]")
    }
}

impl Serialize for TypeSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(ty) = self.single() {
            return serializer.serialize_str(ty.as_str());
        }
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for ty in self.iter() {
            seq.serialize_element(ty.as_str())?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for TypeSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(TypeSetVisitor)
    }
}

/// Reads `type` as a type name or an array of type names.
struct TypeSetVisitor;

impl<'de> Visitor<'de> for TypeSetVisitor {
    type Value = TypeSet;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a type name or an array of type names")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<TypeSet, E> {
        v.parse::<JsonType>().map(TypeSet::from).map_err(E::custom)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<TypeSet, A::Error> {
        let mut set: TypeSet = TypeSet::empty();
        while let Some(name) = seq.next_element::<TypeName>()? {
            set.insert(name.0);
        }
        Ok(set)
    }
}

/// One entry of a `type` array.
struct TypeName(JsonType);

impl<'de> Deserialize<'de> for TypeName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TypeNameVisitor;
        impl Visitor<'_> for TypeNameVisitor {
            type Value = TypeName;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a type name")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<TypeName, E> {
                v.parse::<JsonType>().map(TypeName).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(TypeNameVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonType, TypeSet, UnknownJsonType};
    use crate::json_schema::IntegerSemantics;
    use serde_json::json;

    fn set(types: &[JsonType]) -> TypeSet {
        types.iter().copied().collect()
    }

    #[test]
    fn deserialize_single_name() {
        let expected: TypeSet = set(&[JsonType::String]);
        let actual: TypeSet = serde_json::from_value(json!("string")).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_keeps_every_entry() {
        let expected: TypeSet = set(&[JsonType::Integer, JsonType::String]);
        let actual: TypeSet = serde_json::from_value(json!(["integer", "string"])).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_collapses_duplicates() {
        let expected: TypeSet = set(&[JsonType::Null]);
        let actual: TypeSet = serde_json::from_value(json!(["null", "null"])).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_empty_array_is_empty_set() {
        let expected: TypeSet = TypeSet::empty();
        let actual: TypeSet = serde_json::from_value(json!([])).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_unknown_name_errors() {
        let expected: String = "unknown type \"strng\", expected one of \"array\", \"boolean\", \
                                \"integer\", \"null\", \"number\", \"object\", \"string\""
            .to_string();
        let actual: String = serde_json::from_value::<TypeSet>(json!("strng"))
            .unwrap_err()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_unknown_name_in_array_errors() {
        let expected: String = "unknown type \"any\", expected one of \"array\", \"boolean\", \
                                \"integer\", \"null\", \"number\", \"object\", \"string\""
            .to_string();
        let actual: String = serde_json::from_value::<TypeSet>(json!(["string", "any"]))
            .unwrap_err()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_non_string_entry_errors() {
        let expected: String = "invalid type: map, expected a type name".to_string();
        let actual: String = serde_json::from_value::<TypeSet>(json!([{}]))
            .unwrap_err()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_number_errors() {
        let expected: String =
            "invalid type: integer `1`, expected a type name or an array of type names".to_string();
        let actual: String = serde_json::from_value::<TypeSet>(json!(1))
            .unwrap_err()
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn serialize_single_as_string() {
        let expected: serde_json::Value = json!("null");
        let actual: serde_json::Value = serde_json::to_value(set(&[JsonType::Null])).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn serialize_several_as_array_in_name_order() {
        let expected: serde_json::Value = json!(["integer", "string"]);
        let actual: serde_json::Value =
            serde_json::to_value(set(&[JsonType::String, JsonType::Integer])).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn serialize_empty_as_empty_array() {
        let expected: serde_json::Value = json!([]);
        let actual: serde_json::Value = serde_json::to_value(TypeSet::empty()).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_type_from_str_round_trips_every_name() {
        let expected: Vec<JsonType> = JsonType::ALL.to_vec();
        let actual: Vec<JsonType> = JsonType::ALL
            .iter()
            .map(|t| t.as_str().parse::<JsonType>().unwrap())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_type_from_str_unknown_errors() {
        let expected: Result<JsonType, UnknownJsonType> =
            Err(UnknownJsonType("String".to_string()));
        let actual: Result<JsonType, UnknownJsonType> = "String".parse::<JsonType>();
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_of_one_type() {
        let expected: Option<JsonType> = Some(JsonType::Object);
        let actual: Option<JsonType> = set(&[JsonType::Object]).single();
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_of_two_types_is_none() {
        let expected: Option<JsonType> = None;
        let actual: Option<JsonType> = set(&[JsonType::Object, JsonType::Null]).single();
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_of_empty_is_none() {
        let expected: Option<JsonType> = None;
        let actual: Option<JsonType> = TypeSet::empty().single();
        assert_eq!(expected, actual);
    }

    #[test]
    fn without_removes_type() {
        let expected: TypeSet = set(&[JsonType::String]);
        let actual: TypeSet = set(&[JsonType::String, JsonType::Null]).without(JsonType::Null);
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_single() {
        let expected: String = "string".to_string();
        let actual: String = set(&[JsonType::String]).to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn display_several() {
        let expected: String = "[null, object]".to_string();
        let actual: String = set(&[JsonType::Object, JsonType::Null]).to_string();
        assert_eq!(expected, actual);
    }

    /// Which of a fixed list of instances match `types`, as their indexes.
    fn matching(types: TypeSet, semantics: IntegerSemantics) -> Vec<usize> {
        let instances: [serde_json::Value; 8] = [
            json!(null),
            json!(true),
            json!(1),
            json!(1.0),
            json!(1.5),
            json!("x"),
            json!([]),
            json!({}),
        ];
        instances
            .iter()
            .enumerate()
            .filter(|(_, v)| types.matches(v, semantics))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn matches_null_only_null() {
        let expected: Vec<usize> = vec![0];
        let actual: Vec<usize> = matching(set(&[JsonType::Null]), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_number_every_number() {
        let expected: Vec<usize> = vec![2, 3, 4];
        let actual: Vec<usize> = matching(set(&[JsonType::Number]), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_integer_numeric_accepts_zero_fraction() {
        let expected: Vec<usize> = vec![2, 3];
        let actual: Vec<usize> = matching(set(&[JsonType::Integer]), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_integer_lexical_rejects_zero_fraction() {
        let expected: Vec<usize> = vec![2];
        let actual: Vec<usize> = matching(set(&[JsonType::Integer]), IntegerSemantics::Lexical);
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_any_member_of_several() {
        let expected: Vec<usize> = vec![1, 5, 6, 7];
        let actual: Vec<usize> = matching(
            set(&[
                JsonType::Boolean,
                JsonType::String,
                JsonType::Array,
                JsonType::Object,
            ]),
            IntegerSemantics::Numeric,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn matches_empty_set_nothing() {
        let expected: Vec<usize> = vec![];
        let actual: Vec<usize> = matching(TypeSet::empty(), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }
}
