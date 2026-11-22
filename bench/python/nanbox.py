import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def nanbox_work(n):
    total = 0.0
    for i in range(1, n + 1):
        x = i / 3.0
        total += x * 2.0 - i // 3.0
    return total

assert nanbox_work(3) > 2.99
assert nanbox_work(100) > 1716.0
measure("nanbox20000", lambda n: nanbox_work(n), 20000, 7, 2)
print("=== Verify: ok ===")
