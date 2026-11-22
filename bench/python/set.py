import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def set_work(n):
    s = set()
    for i in range(n):
        s.add(i % 1000)
    return sum(i for i in range(1000) if i in s)

assert set_work(10) == 45
assert set_work(1000) == 499500
measure("set5000", lambda n: set_work(n), 5000, 7, 2)
print("=== Verify: ok ===")
