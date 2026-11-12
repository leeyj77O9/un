// Smoke test for unvm_playground.wasm — loads the module with the documented ABI
// and runs the fixture scripts through it.
import { readFile } from "node:fs/promises";
import { join } from "node:path";

const repoRoot = new URL("../..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const wasmPath = join(repoRoot, "rust", "target", "wasm32-unknown-unknown", "release", "unvm_playground.wasm");
const bytes = await readFile(wasmPath);

const { instance } = await WebAssembly.instantiate(bytes, {});
const exp = instance.exports;

if (typeof exp.playground_alloc !== "function") throw new Error("playground_alloc export missing");
if (typeof exp.playground_run !== "function") throw new Error("playground_run export missing");
if (typeof exp.playground_free !== "function") throw new Error("playground_free export missing");

const mem = () => new Uint8Array(exp.memory.buffer);
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function run(code, fileName = "playground.un") {
    const codeBytes = encoder.encode(code);
    const fileBytes = encoder.encode(fileName);
    const codePtr = exp.playground_alloc(codeBytes.length);
    const filePtr = exp.playground_alloc(fileBytes.length);
    mem().set(codeBytes, codePtr);
    mem().set(fileBytes, filePtr);

    const outLenPtr = exp.playground_alloc(4);
    const resultPtr = exp.playground_run(codePtr, codeBytes.length, filePtr, fileBytes.length, 0, 0, outLenPtr);
    const outLen = new Uint32Array(exp.memory.buffer, outLenPtr, 1)[0];
    const json = decoder.decode(mem().subarray(resultPtr, resultPtr + outLen));

    exp.playground_free(codePtr, codeBytes.length);
    exp.playground_free(filePtr, fileBytes.length);
    exp.playground_free(resultPtr, outLen);
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

const r1 = run("use io\nio.write(\"hello\")");
expect("stdout capture", r1.stdout, "hello\n");

const r2 = run("use io\nio.write(undefined_variable, \"x\")");
expect("script error exit code", r2.exitCode, 1);
expect("script error stdout", r2.stdout, "");
if (r2.error && r2.error.includes("undefined variable 'undefined_variable'")) ok("script error message");
else fail("script error message", `got: ${JSON.stringify(r2.error)}`);

const r3 = run("use io\nuse math\nio.write(math.sqrt(16), \"\\n\")");
expect("math module allowed", r3.stdout, "4 \n\n");
expect("math module exit code", r3.exitCode, 0);
expect("math module error", r3.error, null);

const r4 = run("use os\n");
expect("allowlist blocks os", r4.exitCode, 1);
if (r4.error && r4.error.includes("not allowed")) ok("allowlist error message");
else fail("allowlist error message", `got: ${JSON.stringify(r4.error)}`);

const r5 = run("use io\nloop");
expect("broken script exit code", r5.exitCode, 1);

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
