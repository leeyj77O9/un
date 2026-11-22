import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def classify(x):
    return [0, 1, 2, -1][x % 4]

def match_work(n):
    total = 0
    for i in range(n):
        total += classify(i)
    return total

assert match_work(4) == 2
assert match_work(100) == 50
measure("match20000", lambda n: match_work(n), 20000, 7, 2)
print("=== Verify: ok ===")
