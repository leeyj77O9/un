// Smoke test for unvm_playground.wasm ??loads the module with the documented ABI
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

function writeBuffer(text) {
    const bytes = encoder.encode(text);
    const ptr = exp.playground_alloc(bytes.length);
    mem().set(bytes, ptr);
    return { ptr, length: bytes.length };
}

function run(code, { fileName = "playground.un", stdin = "", allowEof = false } = {}) {
    const codeBytes = writeBuffer(code);
    const fileBytes = writeBuffer(fileName);
    const allowedBytes = writeBuffer("");
    const stdinBytes = writeBuffer(stdin);

    const outLenPtr = exp.playground_alloc(4);
    const resultPtr = exp.playground_run(
        codeBytes.ptr, codeBytes.length,
        fileBytes.ptr, fileBytes.length,
        allowedBytes.ptr, allowedBytes.length,
        stdinBytes.ptr, stdinBytes.length,
        allowEof ? 1 : 0,
        outLenPtr,
    );
    const outLen = new Uint32Array(exp.memory.buffer, outLenPtr, 1)[0];
    const json = decoder.decode(mem().subarray(resultPtr, resultPtr + outLen));

    exp.playground_free(codeBytes.ptr, codeBytes.length);
    exp.playground_free(fileBytes.ptr, fileBytes.length);
    exp.playground_free(allowedBytes.ptr, allowedBytes.length);
    exp.playground_free(stdinBytes.ptr, stdinBytes.length);
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

const r6 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)");
expect("read asks for input", r6.needsInput, true);
expect("read prompt is kept", r6.stdout, "Name: ");
expect("read needs no error", r6.error, null);

const r7 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)", { stdin: "kim\n" });
expect("replayed stdin completes", r7.needsInput, false);
expect("replayed stdin stdout", r7.stdout, "Name: hi kim\n");
expect("replayed stdin exit code", r7.exitCode, 0);

const r8 = run("use io\nname = io.read(\"Name: \")\nio.write(\"hi\", name)", { allowEof: true });
expect("allowEof reads empty line", r8.needsInput, false);
expect("allowEof stdout", r8.stdout, "Name: hi \n");

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
