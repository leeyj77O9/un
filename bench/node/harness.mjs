// 벤치마크 측정 도구 (Node.js).
// Un의 bench/harness.un과 같은 형식, 같은 정의를 사용한다.
// 출력 형식(scripts/bench.ps1이 파싱한다):
//   BENCH|label|iters|reps|warmup|min|median|avg|max|stddev|ops
//
// 각 측정은 warmup번 준비 실행 후, reps번 실행을 하나씩 performance.now()로 잰다.

export function measure(label, work, iters, reps, warmup) {
    for (let i = 0; i < warmup; i++) {
        work(iters);
    }

    const times = [];
    const heapBefore = process.memoryUsage().heapUsed;
    for (let r = 0; r < reps; r++) {
        const t0 = performance.now();
        work(iters);
        times.push(performance.now() - t0);
    }

    times.sort((a, b) => a - b);
    const n = times.length;
    const mn = times[0];
    const mx = times[n - 1];
    const md = times[Math.floor(n / 2)];

    const avg = times.reduce((a, b) => a + b, 0) / n;
    const sd = Math.sqrt(times.reduce((a, b) => a + (b - avg) * (b - avg), 0) / n);
    const ops = iters / (md / 1000);

    console.log(`BENCH|${label}|${iters}|${reps}|${warmup}|${mn}|${md}|${avg}|${mx}|${sd}|${ops}`);

    // 메모리는 Node 측에서만 측정할 수 있다(Un에 메모리 API가 없다).
    // GC 시점이 제어되지 않으므로 힙 사용량의 증가분만 참고 값으로 남긴다.
    const heapAfter = process.memoryUsage().heapUsed;
    console.log(`MEM|${label}|${((heapAfter - heapBefore) / 1048576).toFixed(3)}|${(heapAfter / 1048576).toFixed(3)}`);
}
