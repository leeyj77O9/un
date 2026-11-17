"""Un의 bench/class.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


class Point:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def norm(self):
        return self.x * self.x + self.y * self.y


def class_work(n):
    total = 0
    for i in range(n):
        p = Point(i, i + 1)
        total += p.norm()
    return total


# sum(i^2 + (i+1)^2) = 2*sum(i^2) + 2*sum(i) + n
assert class_work(3) == 19, "class_work(3)"
assert class_work(1000) == 666667000, "class_work(1000)"

measure("class1000", lambda n: class_work(1000), 1000, 7, 2)

print("=== Verify: ok ===")
