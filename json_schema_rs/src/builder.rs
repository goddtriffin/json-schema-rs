//! The runtime of generated struct builders ([`CodeGenSettings::builders`](crate::CodeGenSettings::builders)):
//! each builder field is a [`BuilderField`], and building reports a [`BuilderError`].

use crate::TypeValidationError;
use std::fmt;

/// A builder's value for one field: not set yet, set, or given a value that did not convert into
/// the field's type.
#[derive(Debug, Clone, Default)]
pub enum BuilderField<T> {
    /// No value was supplied.
    #[default]
    Unset,
    /// The supplied value, converted into the field's type.
    Set(T),
    /// The supplied value did not convert: the conversion's error message.
    Invalid(String),
}

impl<T> BuilderField<T> {
    /// The field set from `value`, converted into the field's type; [`Invalid`](Self::Invalid)
    /// with the conversion's message when it does not convert.
    pub fn convert<V>(value: V) -> Self
    where
        V: TryInto<T>,
        V::Error: fmt::Display,
    {
        match value.try_into() {
            Ok(value) => BuilderField::Set(value),
            Err(error) => BuilderField::Invalid(error.to_string()),
        }
    }

    /// The value of the required field `field`.
    ///
    /// # Errors
    ///
    /// The field is unset, or its value did not convert.
    pub fn required(self, field: &'static str) -> Result<T, BuilderFieldError> {
        match self {
            BuilderField::Set(value) => Ok(value),
            BuilderField::Unset => Err(BuilderFieldError::new(field, BuilderProblem::Missing)),
            BuilderField::Invalid(message) => Err(BuilderFieldError::new(
                field,
                BuilderProblem::Conversion(message),
            )),
        }
    }

    /// The value of the optional field `field`: `None` (absent) when unset.
    ///
    /// # Errors
    ///
    /// The field's value did not convert.
    pub fn optional(self, field: &'static str) -> Result<Option<T>, BuilderFieldError> {
        match self {
            BuilderField::Unset => Ok(None),
            other => other.required(field).map(Some),
        }
    }

    /// The value of the field `field`, whose type's default stands for absent (an empty map, an
    /// absent [`Nullable`](crate::Nullable)): that default when unset.
    ///
    /// # Errors
    ///
    /// The field's value did not convert.
    pub fn defaulted(self, field: &'static str) -> Result<T, BuilderFieldError>
    where
        T: Default,
    {
        match self {
            BuilderField::Unset => Ok(T::default()),
            other => other.required(field),
        }
    }
}

/// An optional field's value: unset when absent.
impl<T> From<Option<T>> for BuilderField<T> {
    fn from(value: Option<T>) -> Self {
        value.map_or(BuilderField::Unset, BuilderField::Set)
    }
}

/// Why a builder could not give one field its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderProblem {
    /// A required field was not set.
    Missing,
    /// The value supplied did not convert into the field's type: the conversion's message.
    Conversion(String),
}

/// A field a builder could not give a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuilderFieldError {
    /// The field's Rust name (its setter's name).
    pub field: &'static str,
    /// What went wrong.
    pub problem: BuilderProblem,
}

impl BuilderFieldError {
    fn new(field: &'static str, problem: BuilderProblem) -> Self {
        // A raw identifier's name (`r#type` is the field `type`).
        let field: &'static str = field.strip_prefix("r#").unwrap_or(field);
        BuilderFieldError { field, problem }
    }
}

impl fmt::Display for BuilderFieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.problem {
            BuilderProblem::Missing => write!(f, "no value supplied for `{}`", self.field),
            BuilderProblem::Conversion(message) => write!(
                f,
                "the value supplied for `{}` did not convert: {message}",
                self.field
            ),
        }
    }
}

/// Why a builder could not build its struct.
#[derive(Debug)]
pub enum BuilderError {
    /// Every field the builder could not give a value, in field order.
    Fields(Vec<BuilderFieldError>),
    /// The struct was built, and does not satisfy its schema (a self-validating struct).
    Invalid(TypeValidationError),
}

impl BuilderError {
    /// The error for the fields among `fields` that failed (`Some`), in order.
    #[must_use]
    pub fn fields<const N: usize>(fields: [Option<BuilderFieldError>; N]) -> Self {
        BuilderError::Fields(fields.into_iter().flatten().collect())
    }
}

impl From<TypeValidationError> for BuilderError {
    fn from(error: TypeValidationError) -> Self {
        BuilderError::Invalid(error)
    }
}

/// Prints every problem, one per line.
impl fmt::Display for BuilderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuilderError::Fields(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{field}")?;
                }
                Ok(())
            }
            BuilderError::Invalid(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for BuilderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuilderError::Invalid(error) => Some(error),
            BuilderError::Fields(_) => None,
        }
    }
}

/// The builder of a generated struct (see
/// [`CodeGenSettings::builders`](crate::CodeGenSettings::builders)), invoked in the generated
/// module's `builder` submodule: the builder struct `Name` (a [`BuilderField`] per field), a
/// setter per field taking any value that converts (`TryInto`) into the field's type, `TryFrom`
/// the builder for the struct `super::Name` (every field error at once, then the schema's
/// validation with `@validated`), `From` the struct for its builder, and `super::Name::builder()`.
///
/// Each field is `required name: Type` (no value is an error), `optional name: Type` (the
/// struct's field is `Option<Type>`, the setter takes a `Type`, and no value is `None`), or
/// `defaulted name: Type` (no value is the type's default, which stands for absent: an empty
/// map, an absent [`Nullable`](crate::Nullable)); types name the parent module's items through
/// `super::`. An optional field is cleared on the struct itself (its fields are public).
///
/// ```
/// mod types {
///     #[derive(Debug, Clone, PartialEq)]
///     pub struct Veg {
///         pub name: String,
///         pub weight: Option<u8>,
///     }
///
///     pub mod builder {
///         json_schema_rs::builder!(Veg {
///             required name: String,
///             optional weight: u8,
///         });
///     }
/// }
///
/// let veg: Result<types::Veg, json_schema_rs::BuilderError> =
///     types::Veg::builder().name("carrot").try_into();
/// assert_eq!(veg.ok(), Some(types::Veg { name: "carrot".to_string(), weight: None }));
///
/// let missing: Result<types::Veg, json_schema_rs::BuilderError> =
///     types::Veg::builder().weight(3_u8).try_into();
/// assert_eq!(
///     missing.map_err(|e| e.to_string()).err().as_deref(),
///     Some("no value supplied for `name`")
/// );
/// ```
#[macro_export]
macro_rules! builder {
    (@validated $name:ident { $($kind:ident $field:ident : $ty:ty),* $(,)? }) => {
        $crate::builder!(@impl [validated] $name { $($kind $field : $ty),* });
    };
    ($name:ident { $($kind:ident $field:ident : $ty:ty),* $(,)? }) => {
        $crate::builder!(@impl [] $name { $($kind $field : $ty),* });
    };
    (@check [validated] $built:ident) => {
        $built.validate()?;
    };
    (@check [] $built:ident) => {};
    (@from optional $value:expr) => {
        $crate::BuilderField::from($value)
    };
    (@from $kind:ident $value:expr) => {
        $crate::BuilderField::Set($value)
    };
    (@impl [$($validated:ident)?] $name:ident { $($kind:ident $field:ident : $ty:ty),* }) => {
        #[doc = concat!("The builder of [`", stringify!($name), "`](super::", stringify!($name), ").")]
        #[derive(Debug, Clone, Default)]
        pub struct $name {
            $($field: $crate::BuilderField<$ty>,)*
        }

        impl $name {
            $(
                #[doc = concat!("Sets `", stringify!($field), "` to `value`, converted into its type.")]
                #[must_use]
                pub fn $field<T>(mut self, value: T) -> Self
                where
                    T: ::core::convert::TryInto<$ty>,
                    T::Error: ::core::fmt::Display,
                {
                    self.$field = $crate::BuilderField::convert(value);
                    self
                }
            )*
        }

        impl super::$name {
            /// A builder of this struct, every field unset.
            #[must_use]
            pub fn builder() -> $name {
                <$name as ::core::default::Default>::default()
            }
        }

        impl ::core::convert::From<super::$name> for $name {
            fn from(value: super::$name) -> Self {
                let _ = &value;
                Self {
                    $($field: $crate::builder!(@from $kind value.$field),)*
                }
            }
        }

        $crate::builder!(@try_from [$($validated)?] $name { $($kind $field : $ty),* });
    };
    (@try_from [$($validated:ident)?] $name:ident {}) => {
        impl ::core::convert::TryFrom<$name> for super::$name {
            type Error = $crate::BuilderError;

            fn try_from(_: $name) -> ::core::result::Result<Self, $crate::BuilderError> {
                let built: Self = Self {};
                $crate::builder!(@check [$($validated)?] built);
                Ok(built)
            }
        }
    };
    (@try_from [$($validated:ident)?] $name:ident { $($kind:ident $field:ident : $ty:ty),+ }) => {
        impl ::core::convert::TryFrom<$name> for super::$name {
            type Error = $crate::BuilderError;

            fn try_from(builder: $name) -> ::core::result::Result<Self, $crate::BuilderError> {
                match ($(builder.$field.$kind(stringify!($field)),)+) {
                    ($(Ok($field),)+) => {
                        let built: Self = Self { $($field,)+ };
                        $crate::builder!(@check [$($validated)?] built);
                        Ok(built)
                    }
                    ($($field,)+) => Err($crate::BuilderError::fields([$($field.err(),)+])),
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{BuilderError, BuilderField, BuilderFieldError, BuilderProblem};

    mod types {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Veg {
            pub name: String,
            pub r#type: String,
            pub count: u8,
            pub weight: Option<u8>,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct Empty {}

        pub mod builder {
            crate::builder!(Veg {
                required name: String,
                required r#type: String,
                required count: u8,
                optional weight: u8,
            });

            crate::builder!(Empty {});
        }
    }

    #[test]
    fn builder_builds_struct_from_set_fields() {
        let actual: Option<types::Veg> = types::Veg::builder()
            .name("carrot")
            .r#type("root")
            .count(2_u16)
            .weight(3_u8)
            .try_into()
            .ok();
        let expected: Option<types::Veg> = Some(types::Veg {
            name: "carrot".to_string(),
            r#type: "root".to_string(),
            count: 2,
            weight: Some(3),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_leaves_unset_optional_field_absent() {
        let actual: Option<types::Veg> = types::Veg::builder()
            .name("carrot")
            .r#type("root")
            .count(2_u8)
            .try_into()
            .ok();
        let expected: Option<types::Veg> = Some(types::Veg {
            name: "carrot".to_string(),
            r#type: "root".to_string(),
            count: 2,
            weight: None,
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_reports_every_missing_and_unconvertible_field() {
        let built: Result<types::Veg, BuilderError> =
            types::Veg::builder().count(300_u16).try_into();
        let actual: Option<Vec<BuilderFieldError>> = match built {
            Err(BuilderError::Fields(fields)) => Some(fields),
            _ => None,
        };
        let expected: Option<Vec<BuilderFieldError>> = Some(vec![
            BuilderFieldError {
                field: "name",
                problem: BuilderProblem::Missing,
            },
            BuilderFieldError {
                field: "type",
                problem: BuilderProblem::Missing,
            },
            BuilderFieldError {
                field: "count",
                problem: BuilderProblem::Conversion(
                    "number too large to fit in target type".to_string(),
                ),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_error_displays_one_field_per_line() {
        let built: Result<types::Veg, BuilderError> =
            types::Veg::builder().count(300_u16).try_into();
        let actual: Option<String> = built.err().map(|error: BuilderError| error.to_string());
        let expected: Option<String> = Some(
            "no value supplied for `name`\nno value supplied for `type`\nthe value supplied for `count` did not convert: number too large to fit in target type"
                .to_string(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_from_struct_rebuilds_the_struct() {
        let veg: types::Veg = types::Veg {
            name: "carrot".to_string(),
            r#type: "root".to_string(),
            count: 2,
            weight: Some(3),
        };
        let builder: types::builder::Veg = veg.into();
        let actual: Option<types::Veg> = builder.count(5_u8).try_into().ok();
        let expected: Option<types::Veg> = Some(types::Veg {
            name: "carrot".to_string(),
            r#type: "root".to_string(),
            count: 5,
            weight: Some(3),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_of_struct_without_fields_builds() {
        let actual: Option<types::Empty> = types::Empty::builder().try_into().ok();
        let expected: Option<types::Empty> = Some(types::Empty {});
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_field_required_rejects_unset() {
        let actual: Result<u8, BuilderFieldError> = BuilderField::<u8>::Unset.required("weight");
        let expected: Result<u8, BuilderFieldError> = Err(BuilderFieldError {
            field: "weight",
            problem: BuilderProblem::Missing,
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_field_optional_is_none_unset() {
        let actual: Result<Option<u8>, BuilderFieldError> =
            BuilderField::<u8>::Unset.optional("weight");
        let expected: Result<Option<u8>, BuilderFieldError> = Ok(None);
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_field_optional_rejects_invalid() {
        let actual: Result<Option<u8>, BuilderFieldError> =
            BuilderField::<u8>::Invalid("not a weight".to_string()).optional("weight");
        let expected: Result<Option<u8>, BuilderFieldError> = Err(BuilderFieldError {
            field: "weight",
            problem: BuilderProblem::Conversion("not a weight".to_string()),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_field_defaulted_defaults_unset() {
        let actual: Result<Vec<u8>, BuilderFieldError> =
            BuilderField::<Vec<u8>>::Unset.defaulted("tags");
        let expected: Result<Vec<u8>, BuilderFieldError> = Ok(Vec::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_field_from_none_is_unset() {
        let actual: Result<Option<u8>, BuilderFieldError> =
            BuilderField::<u8>::from(None).optional("weight");
        let expected: Result<Option<u8>, BuilderFieldError> = Ok(None);
        assert_eq!(expected, actual);
    }
}
