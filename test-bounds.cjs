// Input-bound tests.
//
// Before these limits, a ~600-level nested tool argument overflowed the wasm
// stack. The trap did not just fail that call: it left the module unusable, so
// every later call on *every* authorizer (and even `new DogwoodAuthorizer`)
// failed. Each oversized input below must instead throw an ordinary
// `DogwoodError`, and the module must keep working afterwards.
//
// The tests deliberately share one module instance: if any bound regresses,
// the trap poisons the module and every later test fails too, loudly.
const dw = require("./pkg/dogwood_wasm.js");

let pass = 0,
  fail = 0;
function check(name, fn) {
  try {
    fn();
    console.log(`  ok   ${name}`);
    pass++;
  } catch (e) {
    console.log(`  FAIL ${name}: ${e && e.message ? e.message : e}`);
    fail++;
  }
}
function assert(cond, msg) {
  if (!cond) throw new Error(msg || "assertion failed");
}
function throwsDogwoodError(fn, pattern) {
  let err = null;
  try {
    fn();
  } catch (e) {
    err = e;
  }
  assert(err !== null, "expected a throw");
  assert(
    err.name === "DogwoodError",
    `expected a DogwoodError, got ${err.name}: ${err.message}`,
  );
  assert(pattern.test(err.message), `message ${JSON.stringify(err.message)} !~ ${pattern}`);
}

const ACTION_SCHEMA = `
namespace Drupe {
    entity User { dept?: String };
    entity Gateway;
    type ReadInput = { document: String, user: String };
    type ReadOutput = { content: String };
    action "Read" appliesTo {
        principal: [User],
        resource: [Gateway],
        context: { input: ReadInput, output: ReadOutput }
    };
}`;
const POLICY = `permit(principal, action == Drupe::Action::"Read", resource)
when { context.input.document like "*report*" };`;

function readEvent(ts, extra) {
  const input = { document: "quarterly report", user: "alice" };
  if (extra !== undefined) input.extra = extra;
  return {
    action: "Drupe::Action::Read",
    timestamp: ts,
    principal: { type: "Drupe::User", id: "alice" },
    resource: { type: "Drupe::Gateway", id: "gw1" },
    context: { input },
  };
}

/** `n` nested objects around a string: { k: { k: … "x" } }. */
function nestedObject(n) {
  let v = "x";
  for (let i = 0; i < n; i++) v = { k: v };
  return v;
}
/** `n` nested arrays: [[[…]]]. */
function nestedArray(n) {
  let v = [];
  for (let i = 1; i < n; i++) v = [v];
  return v;
}

/** The module still works: a fresh instance decides, and a batch op runs. */
function assertModuleUsable() {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  const d = auth.isAuthorized(readEvent(1));
  assert(d && d.allowed === true, "a fresh instance still decides");
  auth.free();
  assert(dw.checkParse(POLICY).policy_count === 1, "batch ops still run");
}

const DEEP = /nested deeper than the limit of 128/;

console.log("Dogwood WASM input-bound tests\n");

check("a 10,000-level nested context value throws, and nothing is poisoned", () => {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  assert(auth.isAuthorized(readEvent(1)).allowed, "baseline decision");
  throwsDogwoodError(() => auth.isAuthorized(readEvent(2, nestedObject(10000))), DEEP);
  // The rejected event was never observed: history and counters are untouched.
  assert(auth.lastTimestamp === 1, "lastTimestamp unchanged, got " + auth.lastTimestamp);
  assert(auth.decisionCount === 1, "decisionCount unchanged");
  const d = auth.isAuthorized(readEvent(3));
  assert(d && d.allowed === true, "the same instance keeps deciding");
  auth.free();
  assertModuleUsable();
});

check("a 10,000-level nested array throws", () => {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  throwsDogwoodError(() => auth.isAuthorized(readEvent(1, nestedArray(10000))), DEEP);
  auth.free();
  assertModuleUsable();
});

check("the live depth limit matches replay's, value for value", () => {
  // The `input` group is depth 0, so `extra` = nestedObject(n) puts its
  // innermost string at depth n + 1. The `.log` parser allows up to depth 128.
  const scope =
    `@1 scope(principal: Drupe::User::"alice", resource: Drupe::Gateway::"gw1") `;
  function logValue(v) {
    if (typeof v === "string") return JSON.stringify(v);
    if (Array.isArray(v)) return `[${v.map(logValue).join(", ")}]`;
    return `{ ${Object.entries(v).map(([k, x]) => `${k}: ${logValue(x)}`).join(", ")} }`;
  }
  for (const [n, make] of [
    [127, nestedObject],
    [128, nestedObject],
    [128, nestedArray],
    [129, nestedArray],
  ]) {
    const extra = make(n);
    const input = { document: "quarterly report", user: "alice", extra };
    const trace = `${scope}request_context(input: ${logValue(input)}) Drupe::Action::"Read"::request()`;
    let replayOk = true;
    try {
      dw.replay(POLICY, ACTION_SCHEMA, trace);
    } catch (e) {
      replayOk = false;
      assert(/nested deeper than the limit of 128/.test(e.message), "replay: " + e.message);
    }
    const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
    let liveOk = true;
    try {
      auth.isAuthorized(readEvent(1, extra));
    } catch (e) {
      liveOk = false;
      assert(DEEP.test(e.message), "live: " + e.message);
    }
    auth.free();
    assert(
      replayOk === liveOk,
      `${make.name}(${n}): replay ${replayOk ? "accepts" : "rejects"}, live ${liveOk ? "accepts" : "rejects"}`,
    );
  }
  // And the boundary is where we think it is.
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  assert(auth.isAuthorized(readEvent(1, nestedObject(127))).allowed, "127 accepted");
  throwsDogwoodError(() => auth.isAuthorized(readEvent(2, nestedObject(128))), DEEP);
  auth.free();
});

check("deep values in logged fields and entity attributes throw too", () => {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  const logged = readEvent(1);
  logged.logged = { input: { deep: nestedObject(10000) } };
  throwsDogwoodError(() => auth.isAuthorized(logged), DEEP);
  const attrs = readEvent(1);
  attrs.entities = [{ type: "Drupe::User", id: "alice", attrs: { dept: nestedObject(10000) } }];
  throwsDogwoodError(() => auth.isAuthorized(attrs), DEEP);
  auth.free();
  assertModuleUsable();
});

check("a deeply nested Map throws (serde-wasm-bindgen reads Maps as records)", () => {
  let v = "x";
  for (let i = 0; i < 10000; i++) v = new Map([["k", v]]);
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  throwsDogwoodError(() => auth.isAuthorized(readEvent(1, v)), DEEP);
  auth.free();
  assertModuleUsable();
});

check("a cyclic value throws instead of looping or overflowing", () => {
  const cyclic = { a: 1 };
  cyclic.self = cyclic;
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  throwsDogwoodError(() => auth.isAuthorized(readEvent(1, cyclic)), DEEP);
  auth.free();
  assertModuleUsable();
});

check("an event with more than 100,000 values throws", () => {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  throwsDogwoodError(
    () => auth.isAuthorized(readEvent(1, new Array(100001).fill(0))),
    /more than the limit of 100000 values/,
  );
  // A large but in-bounds array is fine.
  assert(auth.isAuthorized(readEvent(2, new Array(10000).fill(0))).allowed, "10,000 values ok");
  auth.free();
});

check("event strings over 16 Mi characters throw; a 1 MiB argument is fine", () => {
  const auth = new dw.DogwoodAuthorizer(POLICY, ACTION_SCHEMA);
  throwsDogwoodError(
    () => auth.isAuthorized(readEvent(1, "x".repeat(16 * 1024 * 1024 + 1))),
    /strings total more than the limit/,
  );
  // Object keys count toward the total too.
  throwsDogwoodError(
    () => auth.isAuthorized(readEvent(1, { ["k".repeat(16 * 1024 * 1024 + 1)]: 1 })),
    /strings total more than the limit/,
  );
  const d = auth.isAuthorized(readEvent(2, "x".repeat(1024 * 1024)));
  assert(d && d.allowed === true, "a 1 MiB tool argument still decides");
  auth.free();
});

check("a 10,000-level nested MCP manifest throws, and nothing is poisoned", () => {
  const manifest = `{"tools":${"[".repeat(10000)}${"]".repeat(10000)}}`;
  throwsDogwoodError(() => dw.mcpToCedarSchema(manifest), /manifest is nested deeper than the limit of 128/);
  assertModuleUsable();
});

check("a manifest nested within the limit is parsed, not rejected", () => {
  // Brackets inside strings do not count toward nesting.
  const manifest = JSON.stringify([
    {
      name: "ReadFile",
      description: "[{".repeat(5000),
      inputSchema: { type: "object", properties: { path: { type: "string" } }, required: ["path"] },
    },
  ]);
  const schema = dw.mcpToCedarSchema(manifest);
  assert(/ReadFile/.test(schema), "schema names the tool");
});

check("a manifest at exactly the depth limit reaches the parser without trapping", () => {
  // 128 levels is allowed through; whatever the parser makes of it, the result
  // must be a value or a DogwoodError, never a trap.
  let schema = { type: "string" };
  for (let i = 0; i < 61; i++) schema = { type: "object", properties: { k: schema } };
  const manifest = JSON.stringify([{ name: "Deep", inputSchema: schema }]);
  let depth = 0, max = 0;
  for (const c of manifest) {
    if (c === "{" || c === "[") max = Math.max(max, ++depth);
    else if (c === "}" || c === "]") depth--;
  }
  assert(max <= 128, "test manifest is within the limit, got " + max);
  try {
    dw.mcpToCedarSchema(manifest);
  } catch (e) {
    assert(e.name === "DogwoodError", `expected a DogwoodError, got ${e.name}: ${e.message}`);
  }
  assertModuleUsable();
});

check("a manifest over 4 MiB throws", () => {
  const manifest = JSON.stringify([{ name: "Big", description: "x".repeat(4 * 1024 * 1024) }]);
  throwsDogwoodError(() => dw.mcpToCedarSchema(manifest), /over the limit of 4194304/);
});

console.log(`\n${pass} passed, ${fail} failed`);
process.exit(fail ? 1 : 0);
