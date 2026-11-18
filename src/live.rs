// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! The **live** (incremental) authorizer exposed to JavaScript as a class:
//! a wasm-bindgen shell over [`dogwood_ops::LiveAuthorizer`], which owns the
//! event contract (validation, rejections, fail-closed decisions).
//!
//! [`replay`](crate::replay) answers "what would this policy have decided over
//! this recorded `.log` trace?". A host that is *making* decisions (an agent
//! gateway, an MCP proxy) needs the other shape: lower once, then feed events
//! **one at a time** as they happen and get a decision back per event, with
//! the temporal history accumulating across calls.
//!
//! ```js
//! const auth = new DogwoodAuthorizer(policySrc, actionSchemaSrc);
//! const d = auth.isAuthorized({
//!   action: 'Drupe::Action::Read',
//!   principal: { type: 'Drupe::User', id: 'alice' },
//!   resource:  { type: 'Drupe::Gateway', id: 'gw1' },
//!   context:   { input: { document: 'quarterly report', user: 'alice' } },
//! });
//! if (!d.allowed) throw new Error('denied');
//! ```
//!
//! The event shape (`EventInput`), the `logged` vs `context` distinction and
//! the remaining gaps are documented on `dogwood_ops::live`.

use dogwood_ops::{EventInput, LiveAuthorizer, OpError};
use wasm_bindgen::prelude::*;

use crate::throw;

// ─── the JS value type ───────────────────────────────────────────────

/// The TS type for a Dogwood field value. Declared as a custom section,
/// because it is recursive and carries the two tagged escapes (`__entity` /
/// `__decimal`) that JSON alone cannot express.
#[wasm_bindgen(typescript_custom_section)]
const TS_VALUE: &'static str = r#"
/**
 * A Dogwood field value. JSON values map to their Dogwood counterparts;
 * non-integer numbers become decimals. Two tagged forms cover what JSON
 * cannot express:
 *
 *  - `{ __entity: { type, id } }` — an entity reference
 *  - `{ __decimal: "1.50" }` — a decimal with exact text preserved (a JS
 *    number would round-trip `1.50` as `1.5`)
 */
export type DogwoodValue =
  | null
  | boolean
  | number
  | string
  | DogwoodValue[]
  | { __entity: EntityRef }
  | { __decimal: string }
  | { [key: string]: DogwoodValue };
"#;

// ─── the exported class ──────────────────────────────────────────────

/// A **stateful, live** Dogwood authorizer: lower a policy set once, then feed
/// events as they happen and get a decision per event, with temporal history
/// accumulating across calls.
///
/// This is the counterpart to the one-shot [`replay`](crate::replay): same
/// engine and same lowering, but the caller drives the loop. Because it holds
/// temporal state, one instance corresponds to one history — see
/// [`reset`](DogwoodAuthorizer::reset).
#[wasm_bindgen]
pub struct DogwoodAuthorizer {
    inner: LiveAuthorizer,
}

#[wasm_bindgen]
impl DogwoodAuthorizer {
    /// Lower `source` against `actionSchema` and build a fresh authorizer with
    /// empty temporal history. Throws a `DogwoodError` (with `.diagnostic`) on
    /// a parse, macro, lowering, or schema failure — the same error contract as
    /// every other operation in these bindings.
    ///
    /// The trailing arguments are the optional service-schema overrides
    /// (event-schema DSL text, `providers.json` text, macro-library text);
    /// omit them for the defaults.
    #[wasm_bindgen(constructor)]
    pub fn new(
        source: &str,
        action_schema: &str,
        event_schema: Option<String>,
        providers: Option<String>,
        macros: Option<String>,
    ) -> Result<DogwoodAuthorizer, JsValue> {
        LiveAuthorizer::new(source, action_schema, event_schema, providers, macros)
            .map(|inner| DogwoodAuthorizer { inner })
            .map_err(throw)
    }

    /// Authorize one event.
    ///
    /// Returns the `AuthorizerDecision` for a decision-kind event (by default,
    /// `kind: "request"`), or `undefined` for a history-only event — which
    /// still updates temporal history, and is how a host records the
    /// `response` half of a call so later policies can correlate against it.
    ///
    /// Throws a `DogwoodError` if the event is malformed (a missing `action`, a
    /// bad `__entity` tag, a wrong-typed field), **too large** (a value nested
    /// deeper than 128 levels, or past the size limits in `bounds`), or
    /// **rejected**: an action or kind the schema does not declare, a
    /// timestamp before the previous event's, or a history event that does not
    /// conform to the event schema (so the authorizer would not record it).
    /// Either way the instance is left **usable** and the temporal history
    /// untouched: a failed call is a no-op and the next call decides normally.
    ///
    /// A *policy* problem is never thrown: an evaluation failure comes back as a
    /// fail-closed `deny` with the cause in `errors`.
    ///
    /// The parameter is taken as a `JsValue` and deserialized here rather than
    /// declared as `EventInput` directly, which is load-bearing: wasm-bindgen
    /// acquires the `&mut self` borrow *before* converting arguments, so a
    /// conversion that fails propagates the exception without releasing the
    /// borrow — permanently poisoning the instance (every later call, `free()`
    /// included, then fails with "recursive use of an object"). Converting
    /// inside the body means the failure is an ordinary `Err` return and the
    /// borrow is released. `unchecked_param_type` keeps the TypeScript
    /// signature `EventInput`.
    #[wasm_bindgen(js_name = isAuthorized, unchecked_return_type = "AuthorizerDecision | undefined")]
    pub fn is_authorized(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "EventInput")] event: JsValue,
    ) -> Result<JsValue, JsValue> {
        // Bound the raw event first: deserializing and converting it both
        // recurse per nesting level, and a stack overflow there would trap and
        // leave the whole module unusable (see `bounds`).
        crate::bounds::check_event(&event)
            .map_err(|e| throw(OpError::message(format!("invalid event: {e}"))))?;
        let event: EventInput = serde_wasm_bindgen::from_value(event)
            .map_err(|e| throw(OpError::message(format!("invalid event: {e}"))))?;
        match self.inner.is_authorized(&event).map_err(throw)? {
            Some(decision) => crate::to_js(&decision),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// Drop all accumulated temporal history and start over, re-lowering the
    /// original sources. Cheaper than constructing a new instance only in that
    /// the caller need not keep the sources around; the lowering cost is the
    /// same. The decision index restarts at 0.
    pub fn reset(&mut self) -> Result<(), JsValue> {
        self.inner.reset().map_err(throw)
    }

    /// How many decisions this instance has returned — the `index` the next
    /// decision will carry.
    #[wasm_bindgen(getter, js_name = decisionCount)]
    pub fn decision_count(&self) -> usize {
        self.inner.decision_count()
    }

    /// The event kinds that are decision points under this instance's event
    /// schema (`["request"]` by default) — the kinds for which
    /// [`isAuthorized`](Self::is_authorized) returns a decision rather than
    /// `undefined`.
    ///
    /// A kind in [`eventKinds`](Self::event_kinds) but not here is history-only:
    /// it is accepted, it updates temporal state, and it yields no verdict.
    #[wasm_bindgen(getter, js_name = decisionKinds)]
    pub fn decision_kinds(&self) -> Vec<String> {
        self.inner.decision_kinds()
    }

    /// Every event kind this instance's event schema declares, decision points
    /// and history-only alike (`["error", "request", "response"]` by default).
    /// Any other kind is rejected — see [`isAuthorized`](Self::is_authorized).
    #[wasm_bindgen(getter, js_name = eventKinds)]
    pub fn event_kinds(&self) -> Vec<String> {
        self.inner.event_kinds()
    }

    /// Every **fully qualified** action id this instance can decide
    /// (`["Drupe::Action::Read", …]`) — the actions the action schema declares.
    /// An event naming anything else is rejected, so a host that maps its own
    /// operation names onto Cedar actions can check the mapping against this
    /// once at startup rather than discovering a gap one denied request at a
    /// time.
    #[wasm_bindgen(getter)]
    pub fn actions(&self) -> Vec<String> {
        self.inner.actions()
    }

    /// The timestamp of the most recent event observed, or `undefined` before
    /// the first (and after a [`reset`](Self::reset)). The next event's
    /// timestamp must be at least this.
    ///
    /// Widened to `f64` so the TypeScript type is `number`, matching
    /// `AuthorizerDecision.timestamp` and the `timestamp` a host passes in.
    /// An `i64` would come back a `BigInt`, which compares false against every
    /// plain number it is likely to be compared with (`5n === 5` is false, and
    /// mixed arithmetic throws) — a needless trap on a value that is a count of
    /// seconds and so nowhere near `f64`'s exact-integer limit.
    #[wasm_bindgen(getter, js_name = lastTimestamp)]
    pub fn last_timestamp(&self) -> Option<f64> {
        self.inner.last_timestamp().map(|t| t as f64)
    }
}
