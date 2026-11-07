// Un의 bench/dict.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function dictWork(n) {
    const d = new Map();
    for (let i = 0; i < n; i++) {
        d.set(i * 2, i);
    }
    let total = 0;
    for (let i = 0; i < n; i++) {
        total += d.get(i * 2);
    }
    return total;
}

// ---------- 검증 ----------
assert.equal(dictWork(5), 10);
assert.equal(dictWork(5000), 12497500);

// ---------- 측정 ----------
measure("dict5000", (n) => dictWork(5000), 10000, 7, 2);

console.log("=== Verify: ok ===");
