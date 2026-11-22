import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
function workInsideModule(n) {
    let total = 0;
    for (let i = 0; i < n; i++) total += i % 13;
    return total;
}
const modimportWork = (n) => workInsideModule(n);
assert.equal(modimportWork(3), 3);
assert.equal(modimportWork(100), 582);
measure("modimport2000", (n) => modimportWork(n), 2000, 7, 2);
console.log("=== Verify: ok ===");
