function edgeInt(n) {
    const a = 0, b = -1, c = 999999999, d = n % 7;
    return a + b + c + d;
}
console.log(`RESULT|int_boundary|${edgeInt(9)}`);
console.log("=== Verify: ok ===");
