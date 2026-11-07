// Un의 bench/fib.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function fib(n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

// ---------- 검증 ----------
assert.equal(fib(10), 55);
assert.equal(fib(24), 46368);

// ---------- 측정 ----------
measure("fib24", (n) => fib(24), 1, 7, 2);

console.log("=== Verify: ok ===");
