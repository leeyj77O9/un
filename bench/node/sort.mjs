// Un의 bench/sort.un과 같은 알고리즘, 같은 입력, 같은 검증.
// Un의 내장 sort에 해당하는 기준은 JS의 Array.prototype.sort다.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function lcg(n, seed) {
    const xs = [];
    let s = seed;
    for (let i = 0; i < n; i++) {
        s = (s * 1103515245 + 12345) % 100000;
        xs.push(s);
    }
    return xs;
}

function qsort(xs) {
    if (xs.length <= 1) {
        return xs;
    }
    const pivot = xs[0];
    const smaller = [];
    const larger = [];
    for (const x of xs.slice(1)) {
        if (x < pivot) {
            smaller.push(x);
        } else {
            larger.push(x);
        }
    }
    const out = qsort(smaller);
    out.push(pivot);
    for (const v of qsort(larger)) {
        out.push(v);
    }
    return out;
}

function isSorted(xs) {
    for (let i = 1; i < xs.length; i++) {
        if (xs[i] < xs[i - 1]) {
            return false;
        }
    }
    return true;
}

function total(xs) {
    return xs.reduce((a, b) => a + b, 0);
}

// ---------- 입력 ----------
const data = lcg(500, 7);
assert.equal(data.length, 500);

// ---------- 검증 ----------
const ref = [...data].sort((a, b) => a - b);
const out = qsort(data);
assert.equal(out.length, 500);
assert.equal(isSorted(out), true);
assert.deepEqual(out, ref);
assert.equal(total(out), total(data));

// ---------- 측정 ----------
measure("qsort500", (n) => qsort(data), 500, 7, 2);

console.log("=== Verify: ok ===");
