"""Frame-level template decoder.

Each byte frame = start bit (0) + 8 data bits MSB first + parity bit
(parity = 1 when the data byte has an even number of 1 bits).
One bit = one cycle (rising edge to rising edge): '1' short, '0' long.
For each frame we try all 256 byte values (with the correct parity bit) and a few
speed factors, predict the rising-edge times, and pick the best match against
the observed rising edges. This tolerates spurious and missing edges.
"""
import numpy as np, sys
from evalz import rises

sr = 48000
zfile = sys.argv[1] if len(sys.argv) > 1 else 'z.npy'
thr = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
z = np.load(zfile).astype(float)
R = rises(z, thr) / sr * 1e6  # microseconds
P = np.diff(R)

T1, T0, TS = 425.0, 1215.0, 1300.0  # 1-cycle, 0-cycle, start-bit cycle
CAP = 250.0

# frame templates: offsets of the 10 rises after the start rise (11 rises incl. start)
vals = np.arange(256)
bits = np.zeros((256, 10), int)
for v in vals:
    b = [(v >> (7 - i)) & 1 for i in range(8)]
    par = 1 if sum(b) % 2 == 0 else 0
    bits[v] = [0] + b + [par]
dur = np.where(bits == 1, T1, T0).astype(float)
dur[:, 0] = TS
offs = np.cumsum(dur, 1)  # rises at end of each bit, relative to start rise
scales = np.linspace(0.94, 1.06, 13)


def best_frame(t0):
    lo, hi = t0 + 100, t0 + offs.max() * 1.08 + 300
    i0, i1 = np.searchsorted(R, [lo, hi])
    obs = R[i0:i1]
    if len(obs) == 0:
        return None
    bestc = None
    for s in scales:
        e = t0 + offs * s  # (256,10)
        # distance from each expected rise to nearest observed rise
        j = np.searchsorted(obs, e)
        jl = np.clip(j - 1, 0, len(obs) - 1); jr = np.clip(j, 0, len(obs) - 1)
        d = np.minimum(np.abs(obs[jl] - e), np.abs(obs[jr] - e))
        cost = np.minimum(d, CAP).sum(1)
        # penalize unexplained observed rises inside frame span
        end = e[:, -1]
        k = np.argmin(cost)
        if bestc is None or cost[k] < bestc[0]:
            bestc = (cost[k], k, s)
    c, v, s = bestc
    e = t0 + offs[v] * s
    # snap end to nearest observed rise if close
    tend = e[-1]
    j = np.argmin(np.abs(obs - tend))
    if abs(obs[j] - tend) < 200:
        tend = obs[j]
    # also: is the 2nd-best value nearly as good? (ambiguity)
    ee = t0 + offs * s
    j2 = np.searchsorted(obs, ee)
    jl = np.clip(j2 - 1, 0, len(obs) - 1); jr = np.clip(j2, 0, len(obs) - 1)
    d = np.minimum(np.minimum(np.abs(obs[jl] - ee), np.abs(obs[jr] - ee)), CAP).sum(1)
    d[v] = np.inf
    return v, c, s, tend, d.min()


def find_data_start():
    k = np.searchsorted(R, 8.0e6)
    # first long cycle after a long run of short cycles
    run = 0
    for i in range(len(P)):
        if P[i] < 700:
            run += 1
        else:
            if run > 1000:
                return i
            run = 0
    raise SystemExit('no leader')


if len(sys.argv) > 3:
    t0 = R[np.searchsorted(R, float(sys.argv[3]) * 1e6)]
else:
    t0 = R[find_data_start()]
print('data starts at %.4fs' % (t0 / 1e6))
out = []; costs = []; amb = []
while True:
    r = best_frame(t0)
    if r is None: break
    v, c, s, tend, c2 = r
    if c > 1500:  # 6 of 10 edges missing -> end of data
        print('stopping at %.3fs, cost %.0f' % (t0 / 1e6, c))
        break
    out.append(v); costs.append(c); amb.append(c2 - c)
    t0 = tend
costs = np.array(costs); amb = np.array(amb)
print('frames', len(out), 'median cost %.0f' % np.median(costs),
      'cost>400:', (costs > 400).sum(), 'ambiguous(margin<150):', (amb < 150).sum())
np.save('frames.npy', np.c_[out, costs, amb])
data = bytes(out)
open('TinyBasic.bin', 'wb').write(data)
for o in range(0, min(len(data), 256), 16):
    print('%04X: %s' % (o, ' '.join('%02X' % x for x in data[o:o + 16])))
