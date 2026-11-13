// Smoke test for unvm_playground.wasm — loads the module with the documented ABI
// and runs the fixture scripts through it. every assertion is deterministic:
// clock values are checked with ranges/regex (the host supplies the time), and
// randomized scripts use a fixed seed.
//
// The wasm imports the host's clock (no OS clock in wasm32-unknown-unknown):
//   env.unvm_monotonic_nanos() -> f64 nanoseconds, e.g. performance.now() * 1e6
//   env.unvm_unix_nanos()      -> f64 nanoseconds, e.g. Date.now() * 1e6
import { readFile } from "node:fs/promises";
import { join } from "node:path";

const repoRoot = new URL("../..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const wasmPath = join(repoRoot, "rust", "target", "wasm32-unknown-unknown", "release", "unvm_playground.wasm");
const bytes = await readFile(wasmPath);

const imports = {
    env: {
        unvm_monotonic_nanos: () => performance.now() * 1e6,
        unvm_unix_nanos: () => Date.now() * 1e6,
    },
};
const { instance } = await WebAssembly.instantiate(bytes, imports);
const exp = instance.exports;

if (typeof exp.playground_alloc !== "function") throw new Error("playground_alloc export missing");
if (typeof exp.playground_run !== "function") throw new Error("playground_run export missing");
if (typeof exp.playground_free !== "function") throw new Error("playground_free export missing");

const mem = () => new Uint8Array(exp.memory.buffer);
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function writeBuffer(text) {
    const bytes = encoder.encode(text);
    if (bytes.length === 0) return { ptr: 0, length: 0 }; // alloc(0) is null; ABI reads null, 0 as empty
    const ptr = exp.playground_alloc(bytes.length);
    mem().set(bytes, ptr);
    return { ptr, length: bytes.length };
}

function freeBuffer(ptr, length) {
    if (length > 0) exp.playground_free(ptr, length);
}

function run(code, { fileName = "playground.un", stdin = "", allowEof = false, allowed = "" } = {}) {
    const codeBuffer = writeBuffer(code);
    const fileBuffer = writeBuffer(fileName);
    const allowedBuffer = writeBuffer(allowed);
    const stdinBuffer = writeBuffer(stdin);

    const outLenPtr = exp.playground_alloc(4);
    const resultPtr = exp.playground_run(
        codeBuffer.ptr, codeBuffer.length,
        fileBuffer.ptr, fileBuffer.length,
        allowedBuffer.ptr, allowedBuffer.length,
        stdinBuffer.ptr, stdinBuffer.length,
        allowEof ? 1 : 0,
        outLenPtr,
    );
    const outLen = new Uint32Array(exp.memory.buffer, outLenPtr, 1)[0];
    const json = decoder.decode(mem().subarray(resultPtr, resultPtr + outLen));

    freeBuffer(codeBuffer.ptr, codeBuffer.length);
    freeBuffer(fileBuffer.ptr, fileBuffer.length);
    freeBuffer(allowedBuffer.ptr, allowedBuffer.length);
    freeBuffer(stdinBuffer.ptr, stdinBuffer.length);
    freeBuffer(resultPtr, outLen);
    exp.playground_free(outLenPtr, 4);

    return JSON.parse(json);
}

let failed = 0;
let passed = 0;
function ok(name) { passed++; console.log(`ok   ${name}`); }
function fail(name, detail) { failed++; console.log(`FAIL ${name}\n  ${detail}`); }
function expect(name, actual, expected) {
    if (actual === expected) ok(name); else fail(name, `expected ${JSON.stringify(expected)}\n  actual   ${JSON.stringify(actual)}`);
}
function expectMatch(name, actual, pattern) {
    if (typeof actual === "string" && pattern.test(actual)) ok(name); else fail(name, `expected match ${pattern}\n  actual   ${JSON.stringify(actual)}`);
}
function joinLines(lines) { return lines.join("\n"); }

// ---------------------------------------------------------------------------
// basic ABI / stdout
// ---------------------------------------------------------------------------

const r1 = run("use io\nio.write(\"hello\")");
expect("stdout capture", r1.stdout, "hello\n");
expect("stdout capture exit code", r1.exitCode, 0);
expect("stdout capture no error", r1.error, null);

const r2 = run("use io\nio.write(undefined_variable, \"x\")");
expect("script error exit code", r2.exitCode, 1);
expect("script error stdout", r2.stdout, "");
if (r2.error && r2.error.includes("undefined variable 'undefined_variable'")) ok("script error message");
else fail("script error message", `got: ${JSON.stringify(r2.error)}`);
expectMatch("script error header has position", r2.error, /<playground>, line \[\d+\], column \[\d+\]/);

// ---------------------------------------------------------------------------
// module allowlist
// ---------------------------------------------------------------------------

const r3 = run("use io\nuse math\nio.write(math.sqrt(16), \"\\n\")");
expect("math module allowed", r3.stdout, "4 \n\n");
expect("math module exit code", r3.exitCode, 0);
expect("math module error", r3.error, null);

const r4 = run("use os\n");
expect("allowlist blocks os", r4.exitCode, 1);
if (r4.error && r4.error.includes("not allowed")) ok("allowlist error message");
else fail("allowlist error message", `got: ${JSON.stringify(r4.error)}`);

const r4b = run("use fs\n");
expect("allowlist blocks fs", r4b.exitCode, 1);
if (r4b.error && r4b.error.includes("not allowed")) ok("allowlist blocks fs message");
else fail("allowlist blocks fs message", `got: ${JSON.stringify(r4b.error)}`);

const r4c = run("use io\nuse random\nrandom.seed(42)\nio.write(random.int(0, 10) >= 0)", { allowed: "io\nrandom" });
expect("custom allowlist permits listed modules", r4c.exitCode, 0);
expect("custom allowlist stdout", r4c.stdout, "true\n");

const r5 = run("use io\nloop");
expect("broken script exit code", r5.exitCode, 1);
expectMatch("broken script parse error message", r5.error, /error:/);

const r5b = run("use io\nio.write(1)\nthis line is not valid syntax unless((");
expectMatch("parse error mentions position", r5b.error, /line \[\d+\], column \[\d+\]/);

// ---------------------------------------------------------------------------
// interactive read() replay
// ---------------------------------------------------------------------------

const r6 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)");
expect("read asks for input", r6.needsInput, true);
expect("read prompt is kept", r6.stdout, "Name: ");
expect("read needs no error", r6.error, null);
expect("read exit code is 1", r6.exitCode, 1);

const r7 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)", { stdin: "kim\n" });
expect("replayed stdin completes", r7.needsInput, false);
expect("replayed stdin stdout", r7.stdout, "Name: hi kim\n");
expect("replayed stdin exit code", r7.exitCode, 0);

const r8 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)", { allowEof: true });
expect("allowEof reads empty line", r8.needsInput, false);
expect("allowEof stdout", r8.stdout, "Name: hi \n");
expect("allowEof exit code", r8.exitCode, 0);

const r8b = run("use io\na = io.read(\"first: \")\nb = io.read(\"second: \")\nio.write(a, b)");
expect("two reads ask for first line", r8b.needsInput, true);
expect("two reads prompt kept", r8b.stdout, "first: ");
const r8c = run("use io\na = io.read(\"first: \")\nb = io.read(\"second: \")\nio.write(a, b)", { stdin: "one\n" });
expect("two reads with one line ask for second", r8c.needsInput, true);
expect("two reads prompt kept", r8c.stdout, "first: second: ");
const r8d = run("use io\na = io.read(\"first: \")\nb = io.read(\"second: \")\nio.write(a, b)", { stdin: "one\ntwo\n" });
expect("two reads with both lines complete", r8d.needsInput, false);
expect("two reads both lines stdout", r8d.stdout, "first: second: one two\n");

// ---------------------------------------------------------------------------
// time APIs (the host supplies the clock)
// ---------------------------------------------------------------------------

const r9 = run("use io\nuse time\nsw = time.stopwatch()\nsw.start()\nsw.stop()\nio.write(sw.ms() >= 0)");
expect("stopwatch exit code", r9.exitCode, 0);
expect("stopwatch runs on host clock", r9.stdout, "true\n");

const r10 = run("use io\nuse time\nio.write(time.epoch() > 0)");
expect("time.epoch exit code", r10.exitCode, 0);
expect("time.epoch is positive", r10.stdout, "true\n");

const r11 = run("use io\nuse time\nio.write(time.epoch_ms() > 0)");
expect("time.epoch_ms is positive", r11.stdout, "true\n");

const r12 = run("use io\nuse time\nio.write(time.monotonic() >= 0.0)");
expect("time.monotonic is non-negative", r12.stdout, "true\n");

const r13 = run("use io\nuse time\nio.write(time.monotonic_ms() >= 0)");
expect("time.monotonic_ms is non-negative", r13.stdout, "true\n");

const r14 = run("use io\nuse time\nio.write(time.now())");
expectMatch("time.now prints a wall clock", r14.stdout, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/);

const r15 = run("use io\nuse time\nd = time.from_epoch(1234567890)\nio.write(d.timestamp())");
expect("time.from_epoch round-trips", r15.stdout, "1234567890\n");

const r16 = run("use io\nuse time\ntime.sleep(1)\nio.write(\"slept\")");
expect("time.sleep does not trap", r16.exitCode, 0);
expect("time.sleep stdout", r16.stdout, "slept\n");

const r17 = run(joinLines([
    "use io",
    "use time",
    "sw = time.stopwatch()",
    "sw.start()",
    "sum = 0",
    "for i in 0..3000000",
    "    sum += i",
    "sw.stop()",
    "io.write(sum)",
    "io.write(sw.ms())",
    "io.write(sw.is_running())",
]));
expect("stopwatch busy loop exit code", r17.exitCode, 0);
expectMatch("stopwatch busy loop sum", r17.stdout.split("\n")[0], /^4499998500000$/);
expectMatch("stopwatch busy loop ms digits", r17.stdout.split("\n")[1], /^\d+$/);
expect("stopwatch busy loop not running", r17.stdout.split("\n")[2], "false");

// ---------------------------------------------------------------------------
// language features
// ---------------------------------------------------------------------------

const r18 = run("use io\nio.write(list((1..5).map(fn(x) -> x * 2)))");
expect("list map display", r18.stdout, "[2, 4, 6, 8]\n");

const r19 = run("use io\nfn fib(n)\n    if n < 2\n        -> n\n    -> fib(n - 1) + fib(n - 2)\nio.write(fib(15))");
expect("recursive fib(15)", r19.stdout, "610\n");

const r20 = run("use io\nio.write({\"a\": 1, \"b\": 2})");
expect("dict display", r20.stdout, "{a:1, b:2}\n");

const r21 = run("use io\nuse iter\nio.write(iter.sorted(list((3, 1, 2))))");
expect("iter.sorted", r21.stdout, "[1, 2, 3]\n");

const r22 = run("use io\nuse iter\nio.write(iter.sum(*list((1..10))))");
expect("iter.sum", r22.stdout, "45\n");

const r23 = run(joinLines([
    "use io",
    "class vec2",
    "    x = 0.0",
    "    y = 0.0",
    "    get point -> (self.x, self.y)",
    "    set x = float(value)",
    "    fn __init__(x, y)",
    "        self.x = x",
    "        self.y = y",
    "",
    "v = vec2(3, 4)",
    "v.x = \"0.1\"",
    "io.write(v.point)",
]));
expect("class property get/set", r23.stdout, "(0.1, 4)\n");

const r24 = run("use io\nio.write(\"unicode: \uD55C\uAE00 \u2713\")");
expect("unicode stdout", r24.stdout, "unicode: \uD55C\uAE00 \u2713\n");

const r25 = run("use io\ntry\n    x = 1 / 0\n    io.write(\"no error\")\ncatch e\n    io.write(\"caught\")");
expect("division by zero is catchable", r25.stdout, "caught\n");
expect("division by zero exit code", r25.exitCode, 0);

// ---------------------------------------------------------------------------
// randomness
// ---------------------------------------------------------------------------

const seeded = "use io\nuse random\nrandom.seed(42)\nfor i in 0..10\n    io.write(random.int(0, 1000))";
const r26a = run(seeded);
const r26b = run(seeded);
expect("seeded random reproduces", r26a.stdout, r26b.stdout);
expectMatch("seeded random prints ten ints", r26a.stdout, /^\d+(\n\d+){9}\n$/);

const r27 = run(joinLines([
    "use io",
    "use random",
    "random.seed(7)",
    "ok = true",
    "for i in 0..100",
    "    v = random.int(1, 6)",
    "    if v < 1",
    "        ok = false",
    "    if v > 6",
    "        ok = false",
    "io.write(ok)",
]));
expect("random.int stays in range", r27.stdout, "true\n");

// ---------------------------------------------------------------------------
// volume / memory
// ---------------------------------------------------------------------------

const r28 = run("use io\nfor i in 0..2000\n    io.write(i)");
expect("2000-line stdout exit code", r28.exitCode, 0);
const lineCount = (r28.stdout.match(/\n/g) || []).length;
expect("2000-line stdout line count", lineCount, 2000);

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
