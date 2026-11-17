"""Un의 bench/regex.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure
import re

words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"]


def build_text(tokens):
    st = 98765
    s = ""
    for i in range(tokens):
        st = (st * 1103515245 + 12345) % 100000
        s += words[st % 6] + str(st % 1000) + " "
    return s


def number_sum(text):
    nums = re.findall(r"[0-9]+", text)
    return sum(int(tok) for tok in nums)


# ---------- 입력 ----------
src = build_text(6000)
assert len(src) == 53813, "the input text is 53813 characters"

# ---------- 검증 ----------
assert len(re.findall(r"[0-9]+", src)) == 6000, "the text holds 6000 numbers"
assert number_sum(src) == 3345400, "the sum of all numbers"
masked = re.sub(r"[0-9]+", "#", src)
assert len(re.findall(r"[0-9]+", masked)) == 0, "replace_all removes every number"

# 한 번의 측정 = 53813자에 [0-9]+ 6000개를 찾는 비용
measure("regex6000", lambda n: len(re.findall(r"[0-9]+", src)), 6000, 7, 2)

print("=== Verify: ok ===")
