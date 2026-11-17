"""Un의 bench/string.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def build(n):
    s = ""
    for i in range(n):
        c = "abcde"[i % 5]
        r = i % 7 + 1
        s += c * r
    return s


def rle_encode(text):
    out = ""
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        run = 1
        while i + run < n and text[i + run] == c:
            run += 1
        out += c + str(run)
        i += run
    return out


def rle_decode(text):
    out = ""
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        digits = ""
        j = i + 1
        while j < n:
            d = text[j]
            if not d.isdigit():
                break
            digits += d
            j += 1
        out += c * int(digits)
        i = j
    return out


# ---------- 입력 ----------
src = build(500)
assert len(src) == 1994, "the input is 1994 characters"

# ---------- 검증 ----------
enc = rle_encode(src)
dec = rle_decode(enc)
assert len(enc) == 1000, "each run encodes to character + digit"
assert dec == src, "decode returns the original string"
assert rle_encode("") == "", "encoding an empty string is empty"
assert rle_decode("") == "", "decoding an empty string is empty"

# ---------- 측정 ----------
measure("rle1994", lambda n: len(rle_decode(rle_encode(src))), 1994, 7, 2)

print("=== Verify: ok ===")
