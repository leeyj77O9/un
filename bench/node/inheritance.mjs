import assert from "node:assert/strict";
import { measure } from "./harness.mjs";
class Animal { constructor(name) { this.name = name; } speak() { return 1; } }
class Dog extends Animal { speak() { return 2; } }
function inheritWork(n) {
    let total = 0;
    for (let i = 0; i < n; i++) { const d = new Dog(`d${i}`); total += d.speak(); }
    return total;
}
assert.equal(inheritWork(3), 6);
assert.equal(inheritWork(1000), 2000);
measure("inherit1000", (n) => inheritWork(n), 1000, 7, 2);
console.log("=== Verify: ok ===");
