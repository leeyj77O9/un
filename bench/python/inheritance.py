import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

class Animal:
    def __init__(self, name): self.name = name
    def speak(self): return 1
class Dog(Animal):
    def speak(self): return 2

def inherit_work(n):
    return sum(Dog(f"d{i}").speak() for i in range(n))

assert inherit_work(3) == 6
assert inherit_work(1000) == 2000
measure("inherit1000", lambda n: inherit_work(n), 1000, 7, 2)
print("=== Verify: ok ===")
