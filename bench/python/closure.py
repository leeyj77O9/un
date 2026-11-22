import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def make_adder(k):
    return lambda x: x + k

def closure_work(n):
    adders = [make_adder(i) for i in range(10)]
    return sum(adders[i % 10](i) for i in range(n))

assert closure_work(3) == 6
assert closure_work(100) == 5400
measure("closure20000", lambda n: closure_work(n), 20000, 7, 2)
print("=== Verify: ok ===")
