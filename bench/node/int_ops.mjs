// Un의 bench/int_ops.un과 동일한 알고리즘, 동일한 입력, 동일한 검증
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function intOps(n) {
    let total = 0;
    for (let i = 1; i <= n; i += 1) {
        total += (i * 1103515245 + 12345) % 10007;
        total += Math.floor(i * 7 / 3);
        total += i & 255;
        total += i ^ 0x5a5a;
    }
    return total;
}

assert.equal(intOps(1), 28799);
assert.equal(intOps(1000), 29272316);
assert.equal(intOps(1000000), 1672061256084);

measure("int_ops", (n) => intOps(n), 1000000, 7, 2);

console.log("=== Verify: ok ===");
