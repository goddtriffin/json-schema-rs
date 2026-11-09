//! Integration test: `json_schema_to_rust!` boxes struct fields whose type is large by value
//! (#120). Every definition here holds three copies of the next, nine deep, so unboxed the root
//! would be about 2 MB by value and deserializing it would overflow a small thread stack; boxed,
//! each struct is small and an instance nested to the bottom reads on a 256 KB stack.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r##"{"title":"Wide Tree","type":"object","properties":{"a":{"$ref":"#/$defs/D0"},"b":{"$ref":"#/$defs/D0"},"c":{"$ref":"#/$defs/D0"}},"additionalProperties":false,"$defs":{"D0":{"type":"object","properties":{"a":{"$ref":"#/$defs/D1"},"b":{"$ref":"#/$defs/D1"},"c":{"$ref":"#/$defs/D1"}},"additionalProperties":false},"D1":{"type":"object","properties":{"a":{"$ref":"#/$defs/D2"},"b":{"$ref":"#/$defs/D2"},"c":{"$ref":"#/$defs/D2"}},"additionalProperties":false},"D2":{"type":"object","properties":{"a":{"$ref":"#/$defs/D3"},"b":{"$ref":"#/$defs/D3"},"c":{"$ref":"#/$defs/D3"}},"additionalProperties":false},"D3":{"type":"object","properties":{"a":{"$ref":"#/$defs/D4"},"b":{"$ref":"#/$defs/D4"},"c":{"$ref":"#/$defs/D4"}},"additionalProperties":false},"D4":{"type":"object","properties":{"a":{"$ref":"#/$defs/D5"},"b":{"$ref":"#/$defs/D5"},"c":{"$ref":"#/$defs/D5"}},"additionalProperties":false},"D5":{"type":"object","properties":{"a":{"$ref":"#/$defs/D6"},"b":{"$ref":"#/$defs/D6"},"c":{"$ref":"#/$defs/D6"}},"additionalProperties":false},"D6":{"type":"object","properties":{"a":{"$ref":"#/$defs/D7"},"b":{"$ref":"#/$defs/D7"},"c":{"$ref":"#/$defs/D7"}},"additionalProperties":false},"D7":{"type":"object","properties":{"a":{"$ref":"#/$defs/D8"},"b":{"$ref":"#/$defs/D8"},"c":{"$ref":"#/$defs/D8"}},"additionalProperties":false},"D8":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{},"f8":{}},"additionalProperties":false}}}"##
);

/// The instance, nested along `a` to the bottom definition.
fn instance() -> serde_json::Value {
    serde_json::json!({"a": {"a": {"a": {"a": {"a": {"a": {"a": {"a": {"a": {"f0": 1, "f8": "leaf"}}}}}}}}}})
}

#[test]
fn nested_instance_round_trips_on_a_small_stack() {
    let expected: serde_json::Value = instance();
    let actual: serde_json::Value = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            let root: schema_0::WideTree = serde_json::from_value(instance()).unwrap();
            serde_json::to_value(&root).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(expected, actual);
}

/// The root's fields hold `D0` boxed (the closure's `Box<schema_0::D0>` only compiles then).
#[test]
fn large_field_is_boxed() {
    let root: schema_0::WideTree = serde_json::from_value(instance()).unwrap();
    let expected: Option<bool> = Some(true);
    let actual: Option<bool> = root.a.map(|d0: Box<schema_0::D0>| d0.a.is_some());
    assert_eq!(expected, actual);
}
