"""
실제 측정 도구 (Node.js의 bench/node/harness.mjs와 동일한 형식/정의).
출력 형식(scripts/bench.ps1이 파싱한다):
  BENCH|label|iters|reps|warmup|min|median|avg|max|stddev|ops
  MEM|label|delta_mb|heap_mb

Warmup을 먼저 돌리고(측정에 포함하지 않음), reps회 각각 시간을 잰 뒤
min/median/avg/max/stddev(모집단 표준편차)와 median 기준 OPS를 출력한다.
검증은 각 스크립트가 스스로 수행하고, 통과 시 "=== Verify: ok ==="를 출력한다.
"""
import math
import time
import tracemalloc

def measure(label, work, iters, reps, warmup):
    for _ in range(warmup):
        work(iters)

    tracemalloc.start()
    tracemalloc.reset_peak()
    heap_before = tracemalloc.get_traced_memory()[0]
    times = []
    for _ in range(reps):
        t0 = time.perf_counter()
        result = work(iters)
        times.append((time.perf_counter() - t0) * 1000.0)  # ms

    # cross-lane 결과 대조용
    print(f"RESULT|{label}|{result}")

    heap_after = tracemalloc.get_traced_memory()[0]
    tracemalloc.stop()

    # 원시 샘플: bench.ps1이 신뢰구간/이상치/변동계수를 계산한다.
    for t in times:
        print(f"SAMPLE|{t}")
    times.sort()
    n = len(times)
    mn = times[0]
    mx = times[n - 1]
    md = times[n // 2]
    avg = sum(times) / n
    var = sum((t - avg) ** 2 for t in times) / n
    sd = math.sqrt(var)
    ops = iters / (md / 1000.0)

    print(f"BENCH|{label}|{iters}|{reps}|{warmup}|{mn}|{md}|{avg}|{mx}|{sd}|{ops}")
    print(f"MEM|{label}|{(heap_after - heap_before) / 1048576.0:.3f}|{heap_after / 1048576.0:.3f}")
