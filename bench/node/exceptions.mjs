// Un bench/exceptions.un과 동일
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
function exceptionWork(n) {
    let total = 0;
    for (let i = 0; i < n; i++) {
        if (i % 5 !== 0) total += i % 7;
    }
    return total;
}
assert.equal(exceptionWork(3), 3);
assert.equal(exceptionWork(100), 234);
measure("except10000", (n) => exceptionWork(n), 10000, 7, 2);
console.log("=== Verify: ok ===");
