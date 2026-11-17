"""Un의 bench/mergesort.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
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


def merge(a, b):
    out = []
    i = 0
    j = 0
    while i < len(a) and j < len(b):
        if a[i] <= b[j]:
            out.append(a[i])
            i += 1
        else:
            out.append(b[j])
            j += 1
    while i < len(a):
        out.append(a[i])
        i += 1
    while j < len(b):
        out.append(b[j])
        j += 1
    return out


def mergesort(xs):
    if len(xs) <= 1:
        return xs
    mid = len(xs) // 2
    return merge(mergesort(xs[:mid]), mergesort(xs[mid:]))


data = lcg(1000, 7)
ref = sorted(data)
out = mergesort(data)
assert len(out) == 1000, "mergesort keeps every element"
assert out == ref, "mergesort output equals the sorted reference"
assert sum(out) == sum(data), "mergesort preserves the sum"

measure("mergesort1000", lambda n: mergesort(data), 1000, 7, 2)

print("=== Verify: ok ===")
