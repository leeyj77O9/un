import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
const classify = (x) => [0, 1, 2, -1][x % 4];
function matchWork(n) {
    let total = 0;
    for (let i = 0; i < n; i++) total += classify(i);
    return total;
}
assert.equal(matchWork(4), 2);
assert.equal(matchWork(100), 50);
measure("match20000", (n) => matchWork(n), 20000, 7, 2);
console.log("=== Verify: ok ===");
