import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
function nanboxWork(n) {
    let total = 0.0;
    for (let i = 1; i <= n; i++) {
        const x = i / 3.0;
        total += x * 2.0 - Math.floor(i / 3.0);
    }
    return total;
}
assert.ok(nanboxWork(3) > 2.99);
assert.ok(nanboxWork(100) > 1716.0);
measure("nanbox20000", (n) => nanboxWork(n), 20000, 7, 2);
console.log("=== Verify: ok ===");
