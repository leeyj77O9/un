"""Un의 bench/loop.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def nested_sum(size):
    total = 0
    for i in range(1, size + 1):
        for j in range(1, size + 1):
            total += i * j
    return total


# sum(i) * sum(j) = (200*201/2)^2 = 20100^2 = 404010000
assert nested_sum(200) == 404010000, "nested product sum"

# 한 번의 nested_sum(200) = 40000번의 곱셈/덧셈
measure("nested_sum_200", lambda n: nested_sum(200), 40000, 7, 2)

print("=== Verify: ok ===")
