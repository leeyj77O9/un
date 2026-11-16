// Un의 bench/matrix.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function matMul(a, b) {
    const n = a.length;
    const out = [];
    for (let i = 0; i < n; i += 1) {
        const row = [];
        for (let j = 0; j < n; j += 1) {
            let s = 0;
            for (let k = 0; k < n; k += 1) {
                s += a[i][k] * b[k][j];
            }
            row.push(s);
        }
        out.push(row);
    }
    return out;
}

function checksum(m) {
    let t = 0;
    for (const row of m) {
        for (const v of row) {
            t += v;
        }
    }
    return t;
}

// ---------- 입력 ----------
const n = 48;
const a = [];
const b = [];
for (let i = 0; i < n; i += 1) {
    const ra = [];
    const rb = [];
    for (let j = 0; j < n; j += 1) {
        ra.push((i * 31 + j * 17 + 7) % 13);
        rb.push((i * 11 + j * 5 + 3) % 11);
    }
    a.push(ra);
    b.push(rb);
}

// ---------- 검증 ----------
const ident = [[1, 0], [0, 1]];
const small = [[1, 2], [3, 4]];
assert.equal(checksum(matMul(small, small)), 54);
assert.deepEqual(matMul(small, ident), small);
assert.equal(checksum(matMul(a, b)), 3319440);

// ---------- 측정 ----------
measure("matrix48", (x) => checksum(matMul(a, b)), 110592, 7, 2);

console.log("=== Verify: ok ===");
