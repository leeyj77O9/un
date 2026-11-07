// Un의 bench/math.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function mathWork(n) {
    let total = 0;
    for (let i = 1; i <= n; i++) {
        total += Math.floor(Math.sqrt(i));
    }
    return total;
}

// ---------- 검증 ----------
assert.equal(mathWork(1), 1);
assert.equal(mathWork(100), 625);
assert.equal(mathWork(1000), 20615);

// ---------- 측정 ----------
measure("math10000", (n) => mathWork(10000), 10000, 7, 2);

console.log("=== Verify: ok ===");
