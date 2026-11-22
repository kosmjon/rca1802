"""Elf II cassette decoding library.

Tape format (Netronics Elf II): one bit = one square-wave cycle, rising edge to
rising edge. '1' = short cycle (~420 us), '0' = long cycle (~1215 us).
Frame = start bit 0 + 8 data bits MSB first + parity bit
(parity = 1 when the data byte has an even number of 1 bits).
The start-bit cycle is slightly longer (~1300 us) due to per-byte software overhead.
"""
import numpy as np, wave
from scipy.signal import butter, filtfilt

sr = 48000
T1, T0, TS = 425.0, 1215.0, 1300.0
CAP = 250.0


def load_y(fn):
    global sr
    w = wave.open(fn); sr = w.getframerate()
    a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
    if w.getnchannels() > 1:
        a = a[::w.getnchannels()]
    b, c = butter(1, 40 / (sr / 2), 'high')
    y = filtfilt(b, c, a)
    return y / np.percentile(np.abs(y), 99)


def rises(z, thr):
    st = z[0] > 0; R = []
    hi = z > thr; lo = z < -thr
    for i in np.nonzero(hi | lo)[0]:
        if not st and hi[i]: st = True; R.append(i)
        elif st and lo[i]: st = False
    return np.array(R)


BITS = np.zeros((256, 10), int)
for v in range(256):
    b = [(v >> (7 - i)) & 1 for i in range(8)]
    BITS[v] = [0] + b + [1 if sum(b) % 2 == 0 else 0]


def offsets(t1=T1, t0=T0, ts=TS):
    dur = np.where(BITS == 1, t1, t0).astype(float)
    dur[:, 0] = ts
    return np.cumsum(dur, 1)


SCALES = np.linspace(0.94, 1.06, 25)


def match(R, t0, offs, scales=SCALES):
    i0, i1 = np.searchsorted(R, [t0 + 100, t0 + offs.max() * 1.08 + 300])
    obs = R[i0:i1]
    if len(obs) == 0:
        return None
    best = None
    for s in scales:
        e = t0 + offs * s
        j = np.searchsorted(obs, e)
        jl = np.clip(j - 1, 0, len(obs) - 1); jr = np.clip(j, 0, len(obs) - 1)
        d = np.minimum(np.abs(obs[jl] - e), np.abs(obs[jr] - e))
        cost = np.minimum(d, CAP).sum(1)
        order = np.argsort(cost)
        if best is None or cost[order[0]] < best[0]:
            best = (cost[order[0]], order[0], s, cost[order[1]] - cost[order[0]])
    c, v, s, margin = best
    e = t0 + offs[v] * s
    # snapped rise times (observed if within 200us, else expected)
    rr = [t0]
    for t in e:
        j = np.argmin(np.abs(obs - t))
        rr.append(obs[j] if abs(obs[j] - t) < 200 else t)
    return dict(val=int(v), cost=float(c), scale=float(s), margin=float(margin),
                rises=rr, bits=BITS[v].tolist())


def decode(z, thr, t_start, stop_cost=1500, offs=None):
    R = rises(z, thr) / sr * 1e6
    if offs is None:
        offs = offsets()
    t0 = R[np.searchsorted(R, t_start * 1e6)]
    frames = []
    while True:
        m = match(R, t0, offs)
        if m is None or m['cost'] > stop_cost:
            break
        frames.append(m)
        t0 = m['rises'][-1]
    return frames
