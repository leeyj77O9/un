import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
const makeAdder = (k) => (x) => x + k;
function closureWork(n) {
    const adders = [];
    for (let i = 0; i < 10; i++) adders.push(makeAdder(i));
    let total = 0;
    for (let i = 0; i < n; i++) total += adders[i % 10](i);
    return total;
}
assert.equal(closureWork(3), 6);
assert.equal(closureWork(100), 5400);
measure("closure20000", (n) => closureWork(n), 20000, 7, 2);
console.log("=== Verify: ok ===");
