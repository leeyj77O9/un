import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def work_inside_module(n):
    return sum(i % 13 for i in range(n))

def modimport_work(n):
    return work_inside_module(n)

assert modimport_work(3) == 3
assert modimport_work(100) == 582
measure("modimport2000", lambda n: modimport_work(n), 2000, 7, 2)
print("=== Verify: ok ===")
