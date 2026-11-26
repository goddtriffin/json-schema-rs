//! The `builder!` macro expanded outside this crate, as generated code expands it: clippy's
//! `allow_attributes` (forbidden in this crate) skips external macros, so the expansion's
//! `#[allow(deprecated)]` is checked here as consumers see it.

use json_schema_rs::{BuilderError, BuilderFieldError, BuilderProblem};

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
        json_schema_rs::builder!(Veg {
            required name: String,
            required r#type: String,
            required count: u8,
            optional weight: u8,
        });

        json_schema_rs::builder!(Empty {});
    }
}

/// A struct with a deprecated field: its builder compiles under `deny(deprecated)`.
#[deny(deprecated)]
mod deprecated_types {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Item {
        pub id: String,
        #[deprecated = "Use id instead."]
        pub name: Option<String>,
    }

    pub mod builder {
        json_schema_rs::builder!(Item {
            required id: String,
            optional name: String,
        });
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
    let built: Result<types::Veg, BuilderError> = types::Veg::builder().count(300_u16).try_into();
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
    let built: Result<types::Veg, BuilderError> = types::Veg::builder().count(300_u16).try_into();
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
#[expect(deprecated, reason = "the expected value names the deprecated field")]
fn builder_round_trips_struct_with_deprecated_field() {
    let item: Option<deprecated_types::Item> = deprecated_types::Item::builder()
        .id("1")
        .name("one")
        .try_into()
        .ok();
    let actual: Option<deprecated_types::Item> = item
        .map(deprecated_types::builder::Item::from)
        .and_then(|builder: deprecated_types::builder::Item| builder.try_into().ok());
    let expected: Option<deprecated_types::Item> = Some(deprecated_types::Item {
        id: "1".to_string(),
        name: Some("one".to_string()),
    });
    assert_eq!(expected, actual);
}
