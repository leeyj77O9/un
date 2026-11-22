// Un bench/str_unicode.un과 동일한 알고리즘/검증
import assert from "node:assert/strict";
import { measure } from "./harness.mjs";

const asciiLen = (s) => s.split("-")[2].length;

function unicodeWork(n) {
    const parts = [];
    for (let i = 0; i < n; i++) {
        const s = `한글-🎉-abc-${i}`;
        parts.push(s.toUpperCase());
    }
    return parts.length * 3 + asciiLen(parts[Math.floor(n / 2)]);
}

assert.equal(unicodeWork(3), 12);
assert.equal(unicodeWork(100), 303);
measure("str_unicode", (n) => unicodeWork(n), 2000, 7, 2);
console.log("=== Verify: ok ===");
