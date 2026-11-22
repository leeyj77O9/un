"""Un bench/str_unicode.un과 동일한 알고리즘/검증"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure

def ascii_len(s):
    return len(s.split("-")[2])

def unicode_work(n):
    parts = []
    for i in range(n):
        s = f"한글-🎉-abc-{i}"
        parts.append(s.upper())
    return len(parts) * 3 + ascii_len(parts[n // 2])

assert unicode_work(3) == 12
assert unicode_work(100) == 303
measure("str_unicode", lambda n: unicode_work(n), 2000, 7, 2)
print("=== Verify: ok ===")
