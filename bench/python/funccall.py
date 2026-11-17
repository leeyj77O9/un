"""Un의 bench/funccall.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def mix(x):
    return (x * 1103515245 + 12345) % 100000


def call_work(n):
    x = 42
    for _ in range(n):
        x = mix(x)
    return x


assert mix(42) == 52635, "mix(42)"
assert call_work(100) == 14870, "call_work(100)"
assert call_work(200000) == 27370, "call_work(200000)"

# 한 번의 call_work = mix() 200000번 호출 + 함수 곱셈/덧셈/나머지
measure("call200000", lambda n: call_work(200000), 200000, 7, 2)

print("=== Verify: ok ===")
