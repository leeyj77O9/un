// Un의 bench/iter.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function iterWork(n) {
    const base = [];
    for (let x = 0; x < n; x++) {
        base.push(x);
    }
    const mapped = base.map((x) => x * 2);
    const evens = mapped.filter((x) => x % 4 === 0);
    return evens.reduce((a, b) => a + b, 0);
}

// ---------- 검증 ----------
assert.equal(iterWork(5), 12);
assert.equal(iterWork(10000), 49990000);

// ---------- 측정 ----------
measure("iter10000", (n) => iterWork(10000), 30000, 7, 2);

console.log("=== Verify: ok ===");
