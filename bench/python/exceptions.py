"""Un bench/exceptions.un과 동일"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def exception_work(n):
    total = 0
    for i in range(n):
        if i % 5 != 0:
            total += i % 7
    return total

assert exception_work(3) == 3
assert exception_work(100) == 234
measure("except10000", lambda n: exception_work(n), 10000, 7, 2)
print("=== Verify: ok ===")
