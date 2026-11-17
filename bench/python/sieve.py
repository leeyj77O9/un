"""Un의 bench/sieve.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def sieve(n):
    flags = [True] * n
    if n > 0:
        flags[0] = False
    if n > 1:
        flags[1] = False
    p = 2
    while p * p < n:
        if flags[p]:
            m = p * p
            while m < n:
                flags[m] = False
                m += p
        p += 1
    return sum(1 for f in flags if f)


assert sieve(2) == 0, "sieve(2) has no primes"
assert sieve(30) == 10, "sieve(30) finds 2,3,5,7,11,13,17,19,23,29"
assert sieve(100) == 25, "sieve(100) finds 25 primes"
assert sieve(30000) == 3245, "sieve(30000) finds 3245 primes"

# 한 번의 sieve(30000) = 30000칸의 배열 초기화 + 배수 제거
measure("sieve30000", lambda n: sieve(30000), 30000, 7, 2)

print("=== Verify: ok ===")
