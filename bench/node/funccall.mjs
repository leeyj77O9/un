// Un의 bench/funccall.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function mix(x) {
    return (x * 1103515245 + 12345) % 100000;
}

function callWork(n) {
    let x = 42;
    for (let i = 0; i < n; i += 1) {
        x = mix(x);
    }
    return x;
}

// ---------- 검증 ----------
// 1103515245는 f64 정밀도 안이라 양쪽 결과가 같다.
assert.equal(mix(42), 52635);
assert.equal(callWork(100), 14870);
assert.equal(callWork(200000), 27370);

// ---------- 측정 ----------
measure("call200000", (n) => callWork(200000), 200000, 7, 2);

console.log("=== Verify: ok ===");
