// Un의 bench/mergesort.un과 동일한 알고리즘, 동일한 입력, 동일한 검증
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function lcg(n, seed) {
    const xs = [];
    let s = seed;
    for (let i = 0; i < n; i += 1) {
        s = (s * 1103515245 + 12345) % 100000;
        xs.push(s);
    }
    return xs;
}

function merge(a, b) {
    const out = [];
    let i = 0;
    let j = 0;
    while (i < a.length && j < b.length) {
        if (a[i] <= b[j]) {
            out.push(a[i]);
            i += 1;
        } else {
            out.push(b[j]);
            j += 1;
        }
    }
    while (i < a.length) {
        out.push(a[i]);
        i += 1;
    }
    while (j < b.length) {
        out.push(b[j]);
        j += 1;
    }
    return out;
}

function mergesort(xs) {
    if (xs.length <= 1) {
        return xs;
    }
    const mid = Math.floor(xs.length / 2);
    return merge(mergesort(xs.slice(0, mid)), mergesort(xs.slice(mid)));
}

const data = lcg(1000, 7);
const ref = [...data].sort((a, b) => a - b);
const out = mergesort(data);
assert.equal(out.length, 1000);
assert.deepEqual(out, ref);
assert.equal(out.reduce((a, b) => a + b, 0), data.reduce((a, b) => a + b, 0));

measure("mergesort1000", (n) => mergesort(data), 1000, 7, 2);

console.log("=== Verify: ok ===");
