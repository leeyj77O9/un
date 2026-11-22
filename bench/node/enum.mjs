import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
const RED = 0, GREEN = 1, BLUE = 2;
function enumWork(n) {
    const colors = [RED, GREEN, BLUE];
    let total = 0;
    for (let i = 0; i < n; i++) {
        const c = colors[i % 3];
        total += c === RED ? 1 : c === GREEN ? 2 : 3;
    }
    return total;
}
assert.equal(enumWork(3), 6);
assert.equal(enumWork(99), 198);
assert.equal(enumWork(100), 199);
measure("enum10000", (n) => enumWork(n), 10000, 7, 2);
console.log("=== Verify: ok ===");
