// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

//! The TypeScript declarations of the report and event types.
//!
//! The Rust types live in `dogwood-ops`, which is host-neutral and so carries
//! no TypeScript derive; their TS shapes are declared here instead, matching
//! their `serde` output under `serde-wasm-bindgen`'s default serializer (the
//! one every function here converts with). The exported functions name these
//! types with `unchecked_return_type` / `unchecked_param_type`.
//!
//! Keep in step with the `serde` shape of the `dogwood_ops` types: a field
//! added, renamed or made optional there must be reflected here. The JS test
//! suites read every field of every report, and `npm run typecheck` checks
//! `example.ts` against these declarations.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_REPORTS: &'static str = r#"
/**
 * A per-policy summary from a successful parse (pre-lowering, schema-free).
 */
export interface PolicySummary {
    /**
     * Number of `temporal { … }` leaves in the policy.
     */
    temporal_count: number;
    /**
     * Whether the policy uses any temporal clause.
     */
    uses_temporal: boolean;
    /**
     * Every information-provider invocation, as `Ns::Fn` keys.
     */
    provider_invocations: string[];
    /**
     * Provider invocations naming a provider not declared in the service
     * schema — a pre-lowering warning surfaced structurally.
     */
    undeclared_providers: string[];
}

/**
 * A structured entity reference, deserialized from `{ type, id }`.
 */
export interface EntityRef {
    /**
     * Entity type, namespace included (`"Drupe::User"`).
     */
    type: string;
    /**
     * The raw, un-escaped entity id.
     */
    id: string;
}

/**
 * One decision point's verdict, with its determining rules and any evaluation
 * errors. History-only events (which yield no decision) contribute no entry.
 */
export interface TimepointVerdict {
    /**
     * 0-based index of this decision point in the decision stream.
     */
    index: number;
    /**
     * The event's wall-clock timestamp.
     */
    timestamp: number;
    verdict: Verdict;
    /**
     * The `.dw` rule indices that determined the decision (empty for an
     * implicit deny).
     */
    determining_rules: number[];
    /**
     * Any evaluation errors folded into this decision (a fail-closed deny
     * carries them here).
     */
    errors: string[];
}

/**
 * One diagnostic finding, owned and serializable — the CLI's projection of
 * a frontend `miette::Diagnostic`. Used for the non-fatal channel (validation
 * errors/warnings); the fatal channel is [`OpError`], which wraps one of
 * these plus the source text for rendering.
 */
export interface Diagnostic {
    severity: Severity;
    /**
     * The diagnostic's stable code (e.g. `"temporal"`), if it has one.
     */
    code?: string;
    message: string;
    /**
     * The labelled spans, in the order the diagnostic reports them. Empty for
     * a span-less finding (e.g. an event-schema error that carries only a
     * message).
     */
    labels: Label[];
    help?: string;
    /**
     * Whether this finding has any source-anchored span. `false` means the CLI
     * cannot underline a snippet for it — only the message is available.
     */
    spanned: boolean;
}

/**
 * One entity supplied to a decision: its identity plus the attributes and
 * direct parents a policy may read (`principal.dept`, `principal in Group`).
 */
export interface EntityInput {
    /**
     * Entity type, namespace included (`"Drupe::User"`).
     */
    type: string;
    /**
     * The raw, un-escaped entity id.
     */
    id: string;
    /**
     * Attributes, `name -> value`. Validated against the action schema at
     * decision time, so a wrong-typed attribute fails closed.
     */
    attrs?: Record<string, DogwoodValue>;
    /**
     * **Direct** parents (`memberOf` edges). Cedar computes the transitive
     * closure, so only direct parents are supplied.
     */
    parents?: EntityRef[];
}

/**
 * One event fed to a [`DogwoodAuthorizer`] — the JS analog of the frontend's
 * `Event`, and a mirror of the `.log` wire format's components.
 */
export interface EventInput {
    /**
     * The **fully qualified** Cedar action id — `"Drupe::Action::Read"`.
     * Must be one of [`actions`](DogwoodAuthorizer::actions), matched as a
     * whole string, so an id containing `:` or `::` (which Cedar permits) is
     * addressed exactly. Any other action is rejected — a bare `"Read"` with
     * the qualified id suggested.
     */
    action: string;
    /**
     * The event kind. `"request"` (the default) is a decision point;
     * `"response"` / `"error"` are history-only — they update temporal state
     * and return `undefined`. Which kinds exist, and which decide, is set by
     * the event schema — see [`eventKinds`](DogwoodAuthorizer::event_kinds) and
     * [`decisionKinds`](DogwoodAuthorizer::decision_kinds).
     *
     * A kind the schema does not declare is **rejected**, with the declared
     * kinds listed, so a typo (`"requst"`) fails loudly.
     */
    kind?: string;
    /**
     * Wall-clock timestamp, in **seconds** — the unit the temporal windows are
     * defined in (`1h` is 3600). From JS that means
     * `Math.floor(Date.now() / 1000)`, *not* `Date.now()`: passing
     * milliseconds silently inflates every window by 1000×, so a `within 1h`
     * predicate would match only events from the last 3.6 seconds.
     *
     * Timestamps order events for the temporal operators, so this must be
     * **non-decreasing** across calls on one instance — one instance is one
     * ordered history. A timestamp before the previous event's is rejected
     * rather than silently producing wrong temporal answers; see
     * [`lastTimestamp`](DogwoodAuthorizer::last_timestamp).
     *
     * **Required**: an event without one is rejected rather than placed at
     * the epoch.
     */
    timestamp: number;
    /**
     * The request principal. Setting it makes the event request-wrapping.
     */
    principal?: EntityRef;
    /**
     * The request resource. Setting it makes the event request-wrapping.
     */
    resource?: EntityRef;
    /**
     * The **durable temporal record**: `group -> { name -> value }`, read by
     * temporal predicates in later events. Distinct from
     * [`context`](Self::context) — see the module docs.
     */
    logged?: Record<string, Record<string, DogwoodValue>>;
    /**
     * The **per-decision Cedar request context**: `group -> { name -> value }`,
     * read by a policy as `context.<group>.<name>`. Never persisted.
     */
    context?: Record<string, Record<string, DogwoodValue>>;
    /**
     * The entity store for this decision — attributes and hierarchy for any
     * entity the policy names.
     */
    entities?: EntityInput[];
}

/**
 * One labelled span within the source a diagnostic points at: a byte range
 * and an optional per-label message. Offsets index the `.dw` (or schema)
 * source text the CLI passed in.
 */
export interface Label {
    /**
     * Byte offset of the label's start in the source.
     */
    start: number;
    /**
     * Byte length of the labelled span.
     */
    len: number;
    /**
     * The label's own message, if it carries one.
     */
    message?: string;
}

/**
 * The Cedar artifacts produced by lowering a `.dw` policy set.
 */
export interface LowerArtifacts {
    /**
     * The lowered Cedar policies, rendered to `.cedar` text.
     */
    cedar_policies: string;
    /**
     * The augmented Cedar action schema, as `.cedarschema` text.
     */
    cedar_schema: string;
    /**
     * The augmented schema in Cedar JSON form.
     */
    cedar_schema_json: string;
    /**
     * Whether the exported Cedar fully reproduces the policy semantics.
     * `false` when temporal/provider fields were hoisted (those need Dogwood
     * at authorize time — the Cedar alone is not self-sufficient).
     */
    self_contained: boolean;
    /**
     * The hoisted temporal leaf ids (`context.<id>` slots), for reference.
     */
    temporal_fields: string[];
    /**
     * The hoisted provider field ids (`context.providers.<id>` slots).
     */
    provider_fields: string[];
    /**
     * The event kinds that are decision points.
     */
    decision_kinds: string[];
}

/**
 * The decision for one event: the verdict plus why.
 */
export interface AuthorizerDecision {
    /**
     * 0-based index of this decision in the authorizer's decision stream
     * (history-only events do not advance it).
     */
    index: number;
    /**
     * The event's timestamp, echoed back.
     */
    timestamp: number;
    verdict: Verdict;
    /**
     * `true` exactly when `verdict === "allow"`: the policy allowed **and**
     * evaluation raised no errors. Stricter than Dogwood's own
     * `Response::allowed`, which is Cedar's decision alone — Cedar skips a
     * rule whose condition errors, so an errored `forbid` would otherwise let
     * a `permit` through.
     */
    allowed: boolean;
    /**
     * The `.dw` rule indices that determined the decision. Empty for an
     * implicit deny (no rule matched) and for a fail-closed deny (`errors`
     * non-empty).
     */
    determining_rules: number[];
    /**
     * Evaluation errors folded into this decision. Any error forces a deny,
     * so a non-empty `errors` means *something went wrong*, not *policy said
     * no*.
     */
    errors: string[];
}

/**
 * The per-timepoint verdict stream from replaying a trace.
 */
export interface ReplayReport {
    verdicts: TimepointVerdict[];
    /**
     * The history events rejected at ingestion (they do not conform to the
     * event schema), in trace order. Absent when every event conforms.
     */
    rejected?: RejectedEvent[];
}

/**
 * A history event the authorizer rejected at ingestion, because it does not
 * conform to the event schema. A rejected event is not ingested, so no later
 * decision sees it; it is not a decision either.
 */
export interface RejectedEvent {
    /**
     * 0-based position of the event in the trace.
     */
    position: number;
    /**
     * The event's wall-clock timestamp.
     */
    timestamp: number;
    /**
     * Why the event was rejected.
     */
    reason: string;
}

/**
 * The result of `check-parse`: the policy set parsed and macro-expanded, with
 * per-policy structural summaries. Syntax and macro errors only — schema-
 * dependent checks (unknown attributes, type errors) are deferred to
 * `validate`.
 */
export interface CheckParseReport {
    policy_count: number;
    policies: PolicySummary[];
}

/**
 * The result of `explain`: the policy set's deterministic informalization —
 * the structured [`Informalization`] (one phrase tree per rule, each phrase
 * carrying its `.dw` span) and the same rendered as indented plain text.
 */
export interface ExplainReport {
    /**
     * The phrase trees (JSON consumers walk this; spans are `[start, end)`
     * byte offsets into the `.dw` source).
     */
    informalization: Informalization;
    /**
     * The plain-text rendering (what human mode prints).
     */
    text: string;
}

/**
 * The result of `validate`: whether the policy set type-checks against the
 * schema, plus every finding. A fatal parse/lower error is returned as the
 * `Err` channel instead; this report covers the type-check phase, which
 * accumulates all findings rather than stopping at the first.
 */
export interface ValidateReport {
    /**
     * True iff there are no validation errors (warnings do not fail).
     */
    passed: boolean;
    /**
     * True iff there are neither errors nor warnings.
     */
    passed_without_warnings: boolean;
    errors: Diagnostic[];
    warnings: Diagnostic[];
}

/**
 * The result of a standalone schema check: whether the artifact is
 * well-formed, and any warnings (a valid-but-suspect schema).
 */
export interface SchemaCheckReport {
    /**
     * The artifact kind that was checked (`"action"`, `"event"`, `"providers"`).
     */
    kind: string;
    /**
     * True iff the artifact parsed and is well-formed.
     */
    ok: boolean;
    /**
     * Non-fatal warnings (from the action-schema and providers checks).
     */
    warnings: string[];
}

/**
 * The severity of a [`Diagnostic`], projected from `miette::Severity`
 * (which defaults to `Error` when a diagnostic does not specify one).
 */
export type Severity = "error" | "warning" | "advice";

/**
 * Whether a decision point allowed or denied — the CLI's own verdict type
 * (not cedar's `Decision`, which is not ours to serialize).
 */
export type Verdict = "allow" | "deny";
"#;
