// Un의 bench/regex.un과 같은 알고리즘, 같은 입력, 같은 검증.
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

const words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];

function buildText(tokens) {
    let st = 98765;
    let s = "";
    for (let i = 0; i < tokens; i += 1) {
        st = (st * 1103515245 + 12345) % 100000;
        s += words[st % 6] + (st % 1000) + " ";
    }
    return s;
}

function numberSum(text) {
    const nums = text.match(/[0-9]+/g);
    let total = 0;
    for (const tok of nums) {
        total += Number(tok);
    }
    return total;
}

// ---------- 입력 ----------
const src = buildText(6000);
assert.equal(src.length, 53813);

// ---------- 검증 ----------
assert.equal((src.match(/[0-9]+/g) || []).length, 6000);
assert.equal(numberSum(src), 3345400);
const masked = src.replace(/[0-9]+/g, "#");
assert.equal((masked.match(/[0-9]+/g) || []).length, 0);

// ---------- 측정 ----------
measure("regex6000", () => (src.match(/[0-9]+/g) || []).length, 6000, 7, 2);

console.log("=== Verify: ok ===");
