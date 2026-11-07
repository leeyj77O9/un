// Un의 bench/loop.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function nestedSum(size) {
    let total = 0;
    for (let i = 1; i <= size; i++) {
        for (let j = 1; j <= size; j++) {
            total += i * j;
        }
    }
    return total;
}

// ---------- 검증 ----------
assert.equal(nestedSum(200), 404010000);

// ---------- 측정 ----------
measure("nested_sum_200", (n) => nestedSum(200), 40000, 7, 2);

console.log("=== Verify: ok ===");
