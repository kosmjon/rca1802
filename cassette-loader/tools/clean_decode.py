"""Decode a clean (emulator-generated) Elf II cassette WAV and report exact timing."""
import sys, numpy as np, collections
from wavload import load

fn = sys.argv[1]
a, sr = load(fn)
if '-inv' in sys.argv: a = -a
pos = a > 0
edges = np.nonzero(pos[1:] != pos[:-1])[0] + 1
lev = pos[edges]                      # level after each edge
t = edges / sr * 1e6
# cycles start at rising edges (level becomes high)
R = t[lev]; Fall = t[~lev]
P = np.diff(R)
H = np.array([Fall[np.searchsorted(Fall, r)] - r for r in R[:-1]])
# leader end: first long cycle after a run of short ones
i = 0; run = 0
while i < len(P):
    if P[i] < 700: run += 1
    else:
        if run > 500: break
        run = 0
    i += 1
print('leader cycles %d, cycle %.1f us (high %.1f)' % (run, np.median(P[i - 100:i]), np.median(H[i - 100:i])))
bits = (P[i:] < 700).astype(int)
nf = len(bits) // 10
out = bytearray(); bad = 0
stats = collections.defaultdict(list)
for f in range(nf):
    fr = bits[f * 10:f * 10 + 10]
    if fr[0] != 0: break
    v = int(''.join(map(str, fr[1:9])), 2)
    if fr[9] != (1 if bin(v).count('1') % 2 == 0 else 0): bad += 1
    out.append(v)
    for k in range(10):
        stats[(k == 0, k == 9, fr[k])].append((P[i + f * 10 + k], H[i + f * 10 + k]))
print('bytes %d parity errors %d' % (len(out), bad))
for k, v in sorted(stats.items()):
    v = np.array(v)
    print('start=%s parity=%s bit=%d: n=%d period %.1f..%.1f high %.1f..%.1f' % (
        k[0], k[1], k[2], len(v), v[:, 0].min(), v[:, 0].max(), v[:, 1].min(), v[:, 1].max()))
import os
open(os.path.splitext(os.path.basename(fn))[0] + '_decoded.bin', 'wb').write(out)
print(out[:32].hex(' '))
