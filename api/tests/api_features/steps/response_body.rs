use cucumber::{gherkin::Step, then};
use pretty_assertions::assert_eq;
use serde_json::Value;

use crate::world::ApiWorld;

fn field(world: &ApiWorld, name: &str) -> Value {
    world.http.last().json::<Value>()[name].clone()
}

#[then(expr = "the response field {string} is:")]
fn the_field_is(world: &mut ApiWorld, name: String, step: &Step) {
    let expected: Value = serde_json::from_str(
        step.docstring
            .as_ref()
            .expect("The expected value must be given as a doc string"),
    )
    .expect("The expected value must be JSON");
    assert_eq!(field(world, &name), expected, "field {name}");
}

#[then(expr = "the response field {string} ends with {string}")]
fn the_field_ends_with(world: &mut ApiWorld, name: String, suffix: String) {
    let value = field(world, &name);
    assert!(
        value.as_str().is_some_and(|text| text.ends_with(&suffix)),
        "field {name} is {value}"
    );
}

#[then(expr = "the response field {string} is not empty")]
fn the_field_is_not_empty(world: &mut ApiWorld, name: String) {
    let value = field(world, &name);
    let empty = match &value {
        Value::Null => true,
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        _ => false,
    };
    assert!(!empty, "field {name} is empty: {value}");
}

#[then(expr = "the response header {string} starts with {string}")]
fn the_header_starts_with(world: &mut ApiWorld, name: String, prefix: String) {
    let header = world.http.last().header(&name).map(str::to_string);
    assert!(
        header
            .as_deref()
            .is_some_and(|value| value.starts_with(&prefix)),
        "header {name} is {header:?}"
    );
}

#[then(expr = "the response header {string} contains {string}")]
fn the_header_contains(world: &mut ApiWorld, name: String, text: String) {
    let header = world.http.last().header(&name).map(str::to_string);
    assert!(
        header.as_deref().is_some_and(|value| value.contains(&text)),
        "header {name} is {header:?}"
    );
}
