// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! Size and nesting limits on untrusted input.
//!
//! Every recursive walk in the bindings and in the crates below them — the
//! `serde-wasm-bindgen` deserializer, [`to_value`](dogwood_ops::live::to_value), Cedar's
//! evaluator, the MCP manifest parser — uses one wasm stack frame per level of
//! input nesting. A few KB of `{"k":{"k":…}}` exhausts that stack, and the
//! resulting trap does not just fail the call: it leaves the module's shadow
//! stack pointer unrestored and the instance's borrow flag set, so **every**
//! later call into the module fails, on every `DogwoodAuthorizer`. One hostile
//! tool argument would be a denial of service for the whole host process.
//!
//! The checks here run **before** any recursive code sees the input, and are
//! themselves iterative (an explicit work stack, no recursion), so oversized
//! input becomes an ordinary thrown `DogwoodError` and the module stays usable.
//!
//! The limits and their accounting live in `dogwood_ops::bounds`, shared with
//! the other bindings; this module is only the walk over JS values. The
//! value-depth limit is the one `dogwood-language`'s `.log` parser already
//! enforces (`MAX_VALUE_DEPTH` in `interpreter/log_parse.rs`), so a value that
//! `replay` accepts is accepted live, and vice versa.

use dogwood_ops::bounds::{EVENT_FRAMING_DEPTH, MAX_VALUE_DEPTH, add_string, admit};
use js_sys::{Array, JsString, Map, Object};
use wasm_bindgen::{JsCast, JsValue};

/// Check a raw JS event against the depth, count and string-length limits
/// before it is deserialized.
///
/// Walks exactly what `serde-wasm-bindgen` would: arrays, `Map`s and plain
/// objects (their own enumerable string-keyed properties). A cyclic object
/// is rejected as too deep rather than looping. This is a coarse, structural
/// bound over the whole event; the exact per-value limit is applied again by
/// `to_value`, once the event's shape is known.
///
/// Both limits are checked when a container's children are *admitted* (see
/// [`admit`]), before any of them is pushed, not when they are popped. So the
/// work stack never holds more than [`MAX_EVENT_VALUES`](dogwood_ops::bounds::MAX_EVENT_VALUES) entries however wide
/// the input is: a 50M-element array is rejected from its length alone,
/// instead of first growing the stack — and wasm linear memory, which never
/// shrinks — by 50M entries, an allocation that could itself fail and trap
/// the module.
pub fn check_event(event: &JsValue) -> Result<(), String> {
    let max_depth = MAX_VALUE_DEPTH + EVENT_FRAMING_DEPTH;
    let mut stack: Vec<(JsValue, usize)> = vec![(event.clone(), 0)];
    let mut values = 1usize; // the root, already on the stack
    let mut string_length = 0usize;

    while let Some((value, depth)) = stack.pop() {
        if let Some(s) = value.dyn_ref::<JsString>() {
            add_string(s.length() as usize, &mut string_length)?;
            continue;
        }
        if !value.is_object() {
            continue; // number, boolean, null, undefined, bigint, symbol
        }

        let child_depth = depth + 1;
        if Array::is_array(&value) {
            let array: &Array = value.unchecked_ref();
            admit(array.length() as usize, child_depth, max_depth, &mut values)?;
            for item in array.iter() {
                stack.push((item, child_depth));
            }
        } else if let Some(map) = value.dyn_ref::<Map>() {
            // A `Map`'s keys are walked as values too.
            let children = (map.size() as usize).saturating_mul(2);
            admit(children, child_depth, max_depth, &mut values)?;
            map.for_each(&mut |v, k| {
                stack.push((k, child_depth));
                stack.push((v, child_depth));
            });
        } else {
            // `Object::entries` builds the entries on the JS heap before their
            // count is known. That memory is garbage-collected, not wasm linear
            // memory, and no larger than the object the caller already holds.
            let entries = Object::entries(value.unchecked_ref::<Object>());
            admit(entries.length() as usize, child_depth, max_depth, &mut values)?;
            for entry in entries.iter() {
                let entry: Array = entry.unchecked_into();
                if let Some(key) = entry.get(0).dyn_ref::<JsString>() {
                    add_string(key.length() as usize, &mut string_length)?;
                }
                stack.push((entry.get(1), child_depth));
            }
        }
    }
    Ok(())
}
