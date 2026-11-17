"""Un의 bench/fib.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def fib(n):
    if n < 2:
        return n
    return fib(n - 1) + fib(n - 2)


assert fib(10) == 55, "fib(10)"
assert fib(24) == 46368, "fib(24)"

# iters=1: 한 번의 fib(24) 재귀 전체가 하나의 측정 단위다.
measure("fib24", lambda n: fib(24), 1, 7, 2)

print("=== Verify: ok ===")
