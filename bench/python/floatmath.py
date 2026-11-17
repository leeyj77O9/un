"""Un의 bench/floatmath.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
import math
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def sin_sum(n):
    total = 0.0
    for i in range(1, n + 1):
        total += math.sin(i)
    return total


# libm 구현 차이로 하위 비트는 달라질 수 있어도 상대 오차 1e-9로 비교한다.
assert abs(sin_sum(20000) - 0.461960217373362) <= 1e-9, "sin(1..20000) sum"
assert abs(sin_sum(10) - 1.4111883712180104) <= 1e-12, "sin(1..10) sum"

measure("sin20000", lambda n: sin_sum(20000), 20000, 7, 2)

print("=== Verify: ok ===")
