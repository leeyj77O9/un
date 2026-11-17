"""Un의 bench/sort.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def lcg(n, seed):
    xs = []
    s = seed
    for i in range(n):
        s = (s * 1103515245 + 12345) % 100000
        xs.append(s)
    return xs


def qsort(xs):
    if len(xs) <= 1:
        return xs
    pivot = xs[0]
    smaller = []
    larger = []
    for x in xs[1:]:
        if x < pivot:
            smaller.append(x)
        else:
            larger.append(x)
    out = qsort(smaller)
    out.append(pivot)
    out.extend(qsort(larger))
    return out


def is_sorted(xs):
    for i in range(1, len(xs)):
        if xs[i] < xs[i - 1]:
            return False
    return True


# ---------- 입력 ----------
data = lcg(500, 7)
assert len(data) == 500, "the input has 500 numbers"

# ---------- 검증 ----------
ref = sorted(data)
out = qsort(data)
assert len(out) == 500, "quicksort keeps every element"
assert is_sorted(out) is True, "quicksort result is ascending"
assert out == ref, "quicksort equals the sorted reference"
assert sum(out) == sum(data), "quicksort preserves the sum"

# ---------- 측정 ----------
measure("qsort500", lambda n: qsort(data), 500, 7, 2)

print("=== Verify: ok ===")
