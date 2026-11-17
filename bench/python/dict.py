"""Un의 bench/dict.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def dict_work(n):
    d = {}
    for i in range(n):
        d[i * 2] = i
    total = 0
    for i in range(n):
        total += d[i * 2]
    return total


# 0..4999의 합 = 12497500
assert dict_work(5) == 10, "small dict work"
assert dict_work(5000) == 12497500, "dict insert and lookup sum"

# 한 번의 dict_work = 5000 삽입 + 5000 조회
measure("dict5000", lambda n: dict_work(5000), 10000, 7, 2)

print("=== Verify: ok ===")
