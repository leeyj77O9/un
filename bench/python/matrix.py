"""Un의 bench/matrix.un과 동일한 알고리즘, 동일한 입력, 동일한 검증."""
import sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from harness import measure


def mat_mul(a, b):
    n = len(a)
    out = []
    for i in range(n):
        row = []
        for j in range(n):
            s = 0
            for k in range(n):
                s += a[i][k] * b[k][j]
            row.append(s)
        out.append(row)
    return out


def checksum(m):
    t = 0
    for row in m:
        for v in row:
            t += v
    return t


# ---------- 입력 ----------
# a[i][j] = (i*31 + j*17 + 7) % 13, b[i][j] = (i*11 + j*5 + 3) % 11
n = 48
a = []
b = []
for i in range(n):
    ra = []
    rb = []
    for j in range(n):
        ra.append((i * 31 + j * 17 + 7) % 13)
        rb.append((i * 11 + j * 5 + 3) % 11)
    a.append(ra)
    b.append(rb)

# ---------- 검증 ----------
ident = [[1, 0], [0, 1]]
small = [[1, 2], [3, 4]]
assert checksum(mat_mul(small, small)) == 54, "2x2 product checksum"
assert mat_mul(small, ident) == small, "multiplying by the identity keeps the matrix"
assert checksum(mat_mul(a, b)) == 3319440, "48x48 product checksum"

# 한 번의 mat_mul = 48^3 = 110592번의 곱셈/덧셈 + 48*48 원소 접근
measure("matrix48", lambda x: checksum(mat_mul(a, b)), 110592, 7, 2)

print("=== Verify: ok ===")
