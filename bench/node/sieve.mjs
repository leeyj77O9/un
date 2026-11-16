// Un의 bench/sieve.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function sieve(n) {
    const flags = new Array(n).fill(true);
    flags[0] = false;
    flags[1] = false;
    for (let p = 2; p * p < n; p += 1) {
        if (!flags[p]) {
            continue;
        }
        for (let m = p * p; m < n; m += p) {
            flags[m] = false;
        }
    }
    let count = 0;
    for (const f of flags) {
        if (f) {
            count += 1;
        }
    }
    return count;
}

// ---------- 검증 ----------
assert.equal(sieve(2), 0);
assert.equal(sieve(30), 10);
assert.equal(sieve(100), 25);
assert.equal(sieve(30000), 3245);

// ---------- 측정 ----------
measure("sieve30000", (n) => sieve(30000), 30000, 7, 2);

console.log("=== Verify: ok ===");
