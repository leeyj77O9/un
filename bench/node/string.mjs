// Un의 bench/string.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

function build(n) {
    let s = "";
    for (let i = 0; i < n; i++) {
        const c = "abcde"[i % 5];
        const r = i % 7 + 1;
        for (let k = 0; k < r; k++) {
            s += c;
        }
    }
    return s;
}

function rleEncode(text) {
    let out = "";
    let i = 0;
    const n = text.length;
    while (i < n) {
        const c = text[i];
        let run = 1;
        while (i + run < n && text[i + run] === c) {
            run += 1;
        }
        out += c + String(run);
        i += run;
    }
    return out;
}

function rleDecode(text) {
    let out = "";
    let i = 0;
    const n = text.length;
    while (i < n) {
        const c = text[i];
        let digits = "";
        let j = i + 1;
        while (j < n) {
            const d = text[j];
            if (!/[0-9]/.test(d)) {
                break;
            }
            digits += d;
            j += 1;
        }
        const count = parseInt(digits, 10);
        for (let k = 0; k < count; k++) {
            out += c;
        }
        i = j;
    }
    return out;
}

// ---------- 입력 ----------
const src = build(500);
assert.equal(src.length, 1994);

// ---------- 검증 ----------
const enc = rleEncode(src);
const dec = rleDecode(enc);
assert.equal(enc.length, 1000);
assert.equal(dec, src);
assert.equal(rleEncode(""), "");
assert.equal(rleDecode(""), "");

// ---------- 측정 ----------
measure("rle1994", (n) => rleDecode(rleEncode(src)).length, 1994, 7, 2);

console.log("=== Verify: ok ===");
