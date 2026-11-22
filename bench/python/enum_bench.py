import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure
from enum import Enum
class Color(Enum):
    RED = 0
    GREEN = 1
    BLUE = 2

def enum_work(n):
    colors = [Color.RED, Color.GREEN, Color.BLUE]
    total = 0
    for i in range(n):
        c = colors[i % 3]
        total += 1 if c == Color.RED else (2 if c == Color.GREEN else 3)
    return total

assert enum_work(3) == 6
assert enum_work(99) == 198
assert enum_work(100) == 199
measure("enum10000", lambda n: enum_work(n), 10000, 7, 2)
print("=== Verify: ok ===")
