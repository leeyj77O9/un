"""Un의 bench/int_ops.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def int_ops(n):
    total = 0
    for i in range(1, n + 1):
        total += (i * 1103515245 + 12345) % 10007
        total += (i * 7) // 3
        total += i & 255
        total += i ^ 0x5a5a
    return total


assert int_ops(1) == 28799, "int_ops(1)"
assert int_ops(1000) == 29272316, "int_ops(1000)"
assert int_ops(1000000) == 1672061256084, "int_ops(1000000)"

measure("int_ops", lambda n: int_ops(n), 1000000, 7, 2)

print("=== Verify: ok ===")
