//! Run-time support for the standard-library impls of generated types: a string enum's JSON
//! names as `&'static str`, `Display` and `FromStr`, all read from the enum's own serde derives
//! so the renames live in one place (see [`string_enum!`](crate::string_enum)), and `Default`
//! from the schema's `default` (see [`schema_default!`](crate::schema_default)).

use serde::de::value::BorrowedStrDeserializer;
use serde::ser::Impossible;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The error of a generated string enum's `FromStr`: the text is not one of the enum's JSON
/// names. Its message lists the names (`unknown variant `x`, expected one of ...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseEnumError {
    message: String,
}

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ParseEnumError {}

/// The JSON name of `value`, a unit variant of an enum deriving `Serialize`.
///
/// # Panics
///
/// When `value` does not serialize as a unit variant (never for a generated string enum).
#[doc(hidden)]
#[must_use]
pub fn variant_name<T: Serialize>(value: &T) -> &'static str {
    value
        .serialize(VariantName)
        .expect("a string enum serializes as a unit variant")
}

/// Parses `text` as one of the JSON names of the enum `T`.
///
/// # Errors
///
/// When `text` is none of them.
#[doc(hidden)]
pub fn parse_variant<'de, T: Deserialize<'de>>(text: &'de str) -> Result<T, ParseEnumError> {
    let deserializer: BorrowedStrDeserializer<'de, serde::de::value::Error> =
        BorrowedStrDeserializer::new(text);
    T::deserialize(deserializer).map_err(|e: serde::de::value::Error| ParseEnumError {
        message: e.to_string(),
    })
}

/// Deserializes the schema `default` `json` of a generated type.
///
/// # Panics
///
/// When `json` does not deserialize into `T`: codegen writes only a value that satisfies the
/// type's schema and fits its Rust type, so this does not happen in generated code.
#[doc(hidden)]
#[must_use]
pub fn schema_default<T: serde::de::DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).expect("the schema's default deserializes into its generated type")
}

/// A serializer that accepts only a unit variant and returns its name.
struct VariantName;

/// Anything [`VariantName`] does not accept.
#[derive(Debug)]
struct NotUnitVariant;

impl fmt::Display for NotUnitVariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a unit variant")
    }
}

impl std::error::Error for NotUnitVariant {}

impl serde::ser::Error for NotUnitVariant {
    fn custom<M: fmt::Display>(_: M) -> Self {
        Self
    }
}

/// The methods of [`VariantName`] that reject a value of their kind.
macro_rules! reject {
    ($($method:ident($($arg:ty),*);)*) => {
        $(
            fn $method(self, $(_: $arg),*) -> Result<Self::Ok, Self::Error> {
                Err(NotUnitVariant)
            }
        )*
    };
}

impl serde::Serializer for VariantName {
    type Ok = &'static str;
    type Error = NotUnitVariant;
    type SerializeSeq = Impossible<&'static str, NotUnitVariant>;
    type SerializeTuple = Impossible<&'static str, NotUnitVariant>;
    type SerializeTupleStruct = Impossible<&'static str, NotUnitVariant>;
    type SerializeTupleVariant = Impossible<&'static str, NotUnitVariant>;
    type SerializeMap = Impossible<&'static str, NotUnitVariant>;
    type SerializeStruct = Impossible<&'static str, NotUnitVariant>;
    type SerializeStructVariant = Impossible<&'static str, NotUnitVariant>;

    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<&'static str, NotUnitVariant> {
        Ok(variant)
    }

    reject! {
        serialize_bool(bool);
        serialize_i8(i8);
        serialize_i16(i16);
        serialize_i32(i32);
        serialize_i64(i64);
        serialize_u8(u8);
        serialize_u16(u16);
        serialize_u32(u32);
        serialize_u64(u64);
        serialize_f32(f32);
        serialize_f64(f64);
        serialize_char(char);
        serialize_str(&str);
        serialize_bytes(&[u8]);
        serialize_none();
        serialize_unit();
        serialize_unit_struct(&'static str);
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _: &T) -> Result<&'static str, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<&'static str, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, NotUnitVariant> {
        Err(NotUnitVariant)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, NotUnitVariant> {
        Err(NotUnitVariant)
    }
}

/// `as_str`, `Display` and `FromStr` for a generated string enum (`pub enum Name { .. }` of unit
/// variants deriving `Serialize` and `Deserialize`), all through its JSON names: `as_str` and
/// `Display` give the name a variant serializes as, and `FromStr` accepts exactly the names it
/// deserializes from.
///
/// ```
/// #[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
/// pub enum Veg {
///     #[serde(rename = "sweet potato")]
///     SweetPotato,
///     #[serde(rename = "pea")]
///     Pea,
/// }
///
/// json_schema_rs::string_enum!(Veg);
///
/// assert_eq!(Veg::SweetPotato.as_str(), "sweet potato");
/// assert_eq!(Veg::Pea.to_string(), "pea");
/// assert_eq!("sweet potato".parse::<Veg>(), Ok(Veg::SweetPotato));
/// assert_eq!(
///     "Pea".parse::<Veg>().map_err(|e| e.to_string()),
///     Err("unknown variant `Pea`, expected `sweet potato` or `pea`".to_string())
/// );
/// ```
#[macro_export]
macro_rules! string_enum {
    ($name:ident) => {
        impl $name {
            /// The JSON name of this variant.
            #[must_use]
            pub fn as_str(&self) -> &'static str {
                $crate::generated_impls::variant_name(self)
            }
        }

        impl ::core::fmt::Display for $name {
            /// Writes the JSON name of this variant.
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::ParseEnumError;

            /// Parses one of the enum's JSON names.
            fn from_str(text: &str) -> Result<Self, $crate::ParseEnumError> {
                $crate::generated_impls::parse_variant(text)
            }
        }
    };
}

/// `impl Default` for a generated type from its schema's `default`, written as JSON. Codegen
/// writes it only for a value that satisfies the type's schema (checked when generating), so the
/// value always deserializes.
///
/// ```
/// #[derive(Debug, PartialEq, serde::Deserialize)]
/// pub struct Veg {
///     pub name: String,
///     pub weight: Option<u8>,
/// }
///
/// json_schema_rs::schema_default!(Veg, r#"{"name":"carrot"}"#);
///
/// assert_eq!(Veg::default(), Veg { name: "carrot".to_string(), weight: None });
/// ```
#[macro_export]
macro_rules! schema_default {
    ($name:ident, $json:literal) => {
        impl ::core::default::Default for $name {
            /// The schema's `default`.
            fn default() -> Self {
                $crate::generated_impls::schema_default($json)
            }
        }
    };
}
