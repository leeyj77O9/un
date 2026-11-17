"""Un의 bench/iter.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def iter_work(n):
    base = list(range(n))
    mapped = [x * 2 for x in base]
    evens = [x for x in mapped if x % 4 == 0]
    return sum(evens)


# 2*i가 4의 배수인 것은 짝수 i이고, 그들의 합이 4 * (0+1+...+4999)
assert iter_work(5) == 12, "small iter work"
assert iter_work(10000) == 49990000, "map + filter + sum"

# 한 번의 iter_work = 10000 생성 + 10000 map + 10000 filter
measure("iter10000", lambda n: iter_work(10000), 30000, 7, 2)

print("=== Verify: ok ===")
