"""Un의 bench/math.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
import math
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def math_work(n):
    total = 0
    for i in range(1, n + 1):
        total += math.floor(math.sqrt(i))
    return total


# n=1: floor(sqrt(1))=1, n=100: k=floor(sqrt(i))의 개수는 2k+1이고, k=10은 1개다.
assert math_work(1) == 1, "math_work(1)"
assert math_work(100) == 625, "math_work(100)"
# n=1000: k=1..30까지의 제곱수 개수 누적 2*sum(k^2)+sum(k) = 19375, k=31은 40개 (31*40)
assert math_work(1000) == 20615, "math_work(1000)"

measure("math10000", lambda n: math_work(10000), 10000, 7, 2)

print("=== Verify: ok ===")
