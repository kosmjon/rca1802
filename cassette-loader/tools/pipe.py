"""Equalize + edge/frame decode, with decision-directed equalizer retraining.

Usage: python pipe.py file.wav
Uses the bootstrap (leader + known first bytes alignment) from decode2.py.
"""
import sys, pickle
import numpy as np
from scipy.linalg import lstsq
from scipy.signal import fftconvolve

WAV = sys.argv[1]
import os
src = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'decode2.py')).read().split('# ---------- bit-level sequential decode')[0]
exec(src)  # gives y, sr, us, t_start, sc, paint, paint_leader, BITS, KNOWN, base ...
import framedec_lib as fl
fl.sr = sr

L = 72


def fit_g(s, mask, n=400000):
    idx = np.nonzero(mask)[0]
    idx = idx[(idx > L) & (idx < len(y) - L)]
    sel = np.random.default_rng(2).choice(idx, size=min(len(idx), n), replace=False)
    X = np.stack([y[sel + k] for k in range(-L, L + 1)], 1)
    g, *_ = lstsq(X, s[sel])
    return g, np.sqrt(np.mean((X @ g - s[sel]) ** 2))


def equalize(g):
    return fftconvolve(y, g[::-1], 'same')


# initial equalizer from leader + known bytes
s0, m0 = build_known(t_start, sc, n_lead=2000)
g, e = fit_g(s0, m0)
print('initial equalizer rms %.3f' % e)
NB = NBYTES
for it in range(5):
    z = equalize(g)
    best = None
    for thr in (0.2, 0.35, 0.5):
        F = fl.decode(z, thr, t_start / 1e6 - 0.00005)
        F = F[:NB]
        c = np.array([f['cost'] for f in F]); m = np.array([f['margin'] for f in F])
        known_ok = bytes(f['val'] for f in F[:len(KNOWN)]) == KNOWN
        score = (len(F) >= NB, -np.median(c) if len(c) else -1e9)
        print('  it %d thr %.2f: frames %d median cost %.0f cost>400 %d margin<150 %d known_ok %s' % (
            it, thr, len(F), np.median(c), (c > 400).sum(), (m < 150).sum(), known_ok), flush=True)
        if best is None or score > best[0]:
            best = (score, thr, F)
    _, thr, F = best
    pickle.dump(F, open(base + '_pipe%d.pkl' % it, 'wb'))
    # retrain equalizer on confident frames
    s = np.zeros_like(y); msk = np.zeros(len(y), bool)
    paint_leader(s, t_start, 2000)
    msk[int((t_start - 0.8e6) * us):int(t_start * us)] = True
    for f in F:
        r = f['rises']
        for k in range(10):
            bt = f['bits'][k]
            paint(s, r[k], [bt], [r[k + 1] - r[k]], [D1 if bt else D0])
        if f['cost'] < 300 and f['margin'] > 200:
            msk[int(r[0] * us):int(r[-1] * us)] = True
    g, e = fit_g(s, msk)
    print(' it %d: chose thr %.2f; retrain rms %.3f on %.1fs' % (it, thr, e, msk.sum() / sr), flush=True)

data = bytes(f['val'] for f in F)
open(base + '_pipe.bin', 'wb').write(data)
with open(base + '_pipe_hex.txt', 'w') as fh:
    for o in range(0, len(data), 16):
        ch = data[o:o + 16]
        fh.write('%04X: %-48s %s\n' % (o, ch.hex(' ').upper(), ''.join(chr(c) if 32 <= c < 127 else '.' for c in ch)))
print('wrote', len(data), 'bytes')
