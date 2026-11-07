// Un의 bench/class.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

class Point {
    constructor(x, y) {
        this.x = x;
        this.y = y;
    }
    norm() {
        return this.x * this.x + this.y * this.y;
    }
}

function classWork(n) {
    let total = 0;
    for (let i = 0; i < n; i++) {
        const p = new Point(i, i + 1);
        total += p.norm();
    }
    return total;
}

// ---------- 검증 ----------
assert.equal(classWork(3), 19);
assert.equal(classWork(1000), 666667000);

// ---------- 측정 ----------
measure("class1000", (n) => classWork(1000), 1000, 7, 2);

console.log("=== Verify: ok ===");
