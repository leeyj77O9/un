// Un의 bench/floatmath.un과 같은 알고리즘, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function sinSum(n) {
    let total = 0.0;
    for (let i = 1; i <= n; i += 1) {
        total += Math.sin(i);
    }
    return total;
}

// ---------- 검증 ----------
// libm 구현 차이로 하위 비트는 달라질 수 있어서 절대 오차로 비교한다.
assert.ok(Math.abs(sinSum(20000) - 0.461960217373362) <= 1e-9, "sin(1..20000) sum");
assert.ok(Math.abs(sinSum(10) - 1.4111883712180104) <= 1e-12, "sin(1..10) sum");

// ---------- 측정 ----------
measure("sin20000", (n) => sinSum(20000), 20000, 7, 2);

console.log("=== Verify: ok ===");
