import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
function setWork(n) {
    const s = new Set();
    for (let i = 0; i < n; i++) s.add(i % 1000);
    let total = 0;
    for (let i = 0; i < 1000; i++) if (s.has(i)) total += i;
    return total;
}
assert.equal(setWork(10), 45);
assert.equal(setWork(1000), 499500);
measure("set5000", (n) => setWork(n), 5000, 7, 2);
console.log("=== Verify: ok ===");
