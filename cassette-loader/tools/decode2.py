"""Waveform-model (MLSE-style) decoder for Netronics Elf II cassette WAV files.

Tape format: one bit = one Q-line square-wave cycle (high then low).
  '1' = short cycle (~430 us), '0' = long cycle (~1215 us).
  Byte frame = start bit 0, 8 data bits MSB first, parity bit
  (parity = 1 when the data byte has an even number of 1 bits).
  The start-bit cycle is a little longer (per-byte software overhead).

Method: the recorded signal is modelled as y = h * s + c, where s is the ideal
+/-1 square wave and h is a linear channel response fitted by least squares on
the leader and the known first bytes. Each frame is decided by synthesising all
256 candidate byte waveforms, passing them through h, and picking the one with
the smallest squared error. The channel is refitted from the confident decode
and the decode repeated.

Usage: python decode2.py file.wav [nbytes] [known-hex-bytes]
"""
import sys, wave, pickle
import numpy as np
from scipy.linalg import lstsq
from scipy.signal import butter, filtfilt, fftconvolve

WAV = sys.argv[1]
NBYTES = int(sys.argv[2], 0) if len(sys.argv) > 2 else 0x0FFF
KNOWN = bytes.fromhex(sys.argv[3]) if len(sys.argv) > 3 else bytes.fromhex('C08000C0F000D00000000000A09A')
import os
base = os.path.splitext(os.path.basename(WAV))[0]  # outputs go to the current directory

w = wave.open(WAV); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
if w.getnchannels() > 1:
    a = a.reshape(-1, w.getnchannels()).mean(1)
b_, c_ = butter(1, 30 / (sr / 2), 'high')
y = filtfilt(b_, c_, a)
y /= np.percentile(np.abs(y), 99)
us = sr / 1e6

T1, T0, TS = 430.0, 1215.0, 1285.0   # nominal cycle lengths (us)
D1, D0 = 0.43, 0.47                  # high fraction of cycle
K1, K2 = 30, 90                      # channel taps: precursor / postcursor

BITS = np.zeros((256, 10), int)
for v in range(256):
    bb = [(v >> (7 - i)) & 1 for i in range(8)]
    BITS[v] = [0] + bb + [1 if sum(bb) % 2 == 0 else 0]
DUR = np.where(BITS == 1, T1, T0).astype(float); DUR[:, 0] = TS
HIF = np.where(BITS == 1, D1, D0)


def square(t_grid, rises, highs):
    """rises: (C, M+1) cycle boundary times; highs: (C, M) high durations.
    Returns (C, N) of +1/-1/0 (0 outside the cycles)."""
    k = (t_grid[None, :, None] >= rises[:, None, :]).sum(2) - 1  # (C,N)
    M = highs.shape[1]
    inside = (k >= 0) & (k < M)
    kc = np.clip(k, 0, M - 1)
    r0 = np.take_along_axis(rises, kc, 1)
    hi = np.take_along_axis(highs, kc, 1)
    out = np.where(t_grid[None, :] - r0 < hi, 1.0, -1.0)
    return np.where(inside, out, 0.0)


def paint(s, t, bits, durs, hif):
    for bt, d, f in zip(bits, durs, hif):
        i0, i1, i2 = (int(round(x * us)) for x in (t, t + f * d, t + d))
        s[i0:i1] = 1; s[i1:i2] = -1; t += d
    return t


def fit_h(s, mask, n=400000):
    idx = np.nonzero(mask)[0]
    idx = idx[(idx > K2 + 1) & (idx < len(y) - K1 - 1)]
    sel = np.random.default_rng(0).choice(idx, size=min(len(idx), n), replace=False)
    X = np.stack([s[sel - k] for k in range(-K1, K2 + 1)] + [np.ones(len(sel))], 1)
    h, *_ = lstsq(X, y[sel])
    return h, np.sqrt(np.mean((X @ h - y[sel]) ** 2))


def conv(S, h):
    """Apply channel: y[n] = sum_{k=-K1..K2} h[k] s[n-k] + c. S shape (C, N)."""
    out = fftconvolve(S, h[None, :-1], mode='full', axes=1)
    return out[:, K1:K1 + S.shape[1]] + h[-1]


# ---------- locate leader and data start ----------
def find_leader():
    # 2400 Hz energy envelope via zero-crossing rate in 20 ms windows
    win = int(0.02 * sr)
    zc = np.abs(np.diff(np.signbit(y).astype(int)))
    n = len(zc) // win
    rate = zc[:n * win].reshape(n, win).sum(1) / 0.02 / 2  # Hz
    amp = np.abs(y[:n * win]).reshape(n, win).mean(1)
    good = (np.abs(rate - 2330) < 150) & (amp > 0.05)
    # longest run of good windows
    best = (0, 0); i = 0
    while i < n:
        if good[i]:
            j = i
            while j < n and good[j]: j += 1
            if j - i > best[1] - best[0]: best = (i, j)
            i = j
        else:
            i += 1
    return best[0] * win / sr, best[1] * win / sr


ls, le = find_leader()
print('leader %.2fs .. %.2fs' % (ls, le))
# measure leader period precisely from zero crossings in its last 2 s
seg = y[int((le - 2.2) * sr):int((le - 0.2) * sr)]
up = np.nonzero((seg[:-1] < 0) & (seg[1:] >= 0))[0]
T1L = np.median(np.diff(up)) / sr * 1e6
print('leader cycle %.1f us' % T1L)

# ---------- bootstrap channel: search alignment of leader + known bytes ----------
# leader cycle anchors: interpolated upward zero crossings of y (Q rising edge
# lies at a fixed offset from these; the channel taps absorb that delay)
_seg0 = int((ls + 0.2) * sr); _seg1 = int((le + 0.05) * sr)
_q = y[_seg0:_seg1]
_u = np.nonzero((_q[:-1] < 0) & (_q[1:] >= 0))[0]
U = (_seg0 + _u + _q[_u] / (_q[_u] - _q[_u + 1])) / us   # us
U = U[np.r_[True, np.diff(U) > 0.6 * T1L]]


def paint_leader(s, t_end, n_lead):
    """Paint leader cycles anchored on U, ending at the last anchor <= t_end - T1L/2."""
    # shift anchors by a constant so the leader's cycle grid ends exactly at t_end
    # (zero crossings lag the Q edges by the channel delay)
    kk = np.searchsorted(U, t_end - 3 * T1L)
    ref = U[max(0, kk - 40):kk]
    ph = np.angle(np.mean(np.exp(2j * np.pi * (ref - t_end) / T1L))) / (2 * np.pi) * T1L
    V = U - ph
    k1 = np.searchsorted(V, t_end - 0.5 * T1L)
    k0 = max(0, k1 - n_lead)
    for k in range(k0, k1):
        nxt = V[k + 1] if k + 1 < len(V) and V[k + 1] < t_end - 0.5 * T1L else t_end
        d = nxt - V[k]
        if d > 1.5 * T1L: d = T1L
        paint(s, V[k], [1], [d], [0.5])
    return V[k0]


def build_known(t_start, sc, n_lead=1500):
    s = np.zeros_like(y); m = np.zeros(len(y), bool)
    a0 = int(paint_leader(s, t_start, n_lead) * us)
    t = t_start
    for v in KNOWN:
        t = paint(s, t, BITS[v], DUR[v] * sc, HIF[v])
    m[a0:int(t * us)] = True
    return s, m

import os
CACHE = base + '_boot.npy'
if os.path.exists(CACHE):
    e, t_start, sc = np.load(CACHE)
else:
    best = None
    # first cycle after the leader that is clearly longer than a leader cycle
    dU = np.diff(U)
    k = np.searchsorted(U, (le - 1.0) * 1e6)
    while k < len(dU) and dU[k] < 1.4 * T1L:
        k += 1
    t_guess = U[k]
    print('leader ends near %.5fs (first long cycle)' % (t_guess / 1e6))
    for t_start in np.arange(t_guess - 1.5 * T1L, t_guess + 0.75 * T1L, T1L / 8):
        s, m = build_known(t_start, 1.0, n_lead=300)
        h, e = fit_h(s, m, n=10000)
        if best is None or e < best[0]:
            best = (e, t_start, 1.0)
    e, t_start, sc = best
    for tt in np.arange(t_start - T1L / 4, t_start + T1L / 4, 5):
        for scx in sc * np.linspace(0.95, 1.05, 21):
            s, m = build_known(tt, scx, n_lead=300)
            h, e = fit_h(s, m, n=10000)
            if e < best[0]:
                best = (e, tt, scx)
    e, t_start, sc = best
    np.save(CACHE, np.array(best))
print('data start %.5fs scale %.4f bootstrap rms %.3f' % (t_start / 1e6, sc, e))
s, m = build_known(t_start, sc)
h, e = fit_h(s, m)
print('initial channel rms %.3f' % e)
print('initial channel rms %.3f' % e)


# ---------- bit-level sequential decode with lookahead ----------
# Each decision compares candidate continuations over the same fixed time window,
# so long ('0') and short ('1') hypotheses are judged on equal terms.
LOOK = 5          # bits of lookahead
W = 1500.0        # us decision window after the current bit start
PRE = 200.0       # us before the current bit included in the window
SCS = np.array([0.97, 0.985, 1.0, 1.015, 1.03])
PATS = np.array([[(p >> (LOOK - 1 - i)) & 1 for i in range(LOOK)] for p in range(2 ** LOOK)])


def decode(h, nbytes, verbose=True):
    s = np.zeros_like(y)
    paint_leader(s, t_start, 3000)
    t = t_start; scale = sc
    out_bits = []                 # committed bits (frame-structured)
    margins = []                  # per committed bit
    starts = []                   # per committed bit start time
    nbits = nbytes * 10
    known_bits = np.concatenate([BITS[v] for v in KNOWN]) if len(KNOWN) else np.zeros(0, int)
    a0n = int(PRE * us); wn = int((PRE + W) * us)
    while len(out_bits) < nbits:
        n = len(out_bits)
        pos = np.arange(n, n + LOOK) % 10
        cand = PATS.copy()
        cand[:, pos == 0] = 0                      # start bits are always 0
        for j in range(LOOK):                     # known bytes forced
            if n + j < len(known_bits):
                cand[:, j] = known_bits[n + j]
        cand = np.unique(cand, axis=0)
        C = len(cand)
        # durations: (C*len(SCS), LOOK); only the current bit uses the trial scale,
        # the lookahead bits use the tracked scale
        base_d = np.where(cand == 1, T1, T0).astype(float)
        base_d[:, pos == 0] = TS
        hif = np.where(cand == 1, D1, D0)
        D = np.repeat(base_d[None], len(SCS), 0) * scale        # (S,C,L)
        D[:, :, 0] *= SCS[:, None]
        D = D.reshape(-1, LOOK); HF = np.tile(hif, (len(SCS), 1)) * D
        r = np.concatenate([np.full((len(D), 1), t), t + np.cumsum(D, 1)], 1)
        g0 = int(round((t - PRE) * us)) - K2
        g1 = g0 + K2 + wn + K1
        tg = np.arange(g0, g1) / us
        S = square(tg, r, HF)
        past = tg < t
        S[:, past] = s[g0:g1][past]
        Y = conv(S, h)[:, K2:K2 + wn]
        ye = y[g0 + K2:g0 + K2 + wn]
        err = ((Y - ye) ** 2).sum(1)
        k = int(np.argmin(err))
        si, ci = divmod(k, C)
        bit = int(cand[ci, 0])
        alt = err[np.repeat(cand[:, 0][None], len(SCS), 0).reshape(-1) != bit]
        margins.append((alt.min() - err[k]) if len(alt) else np.inf)
        d0 = D[k, 0]
        paint(s, t, [bit], [d0], [D1 if bit else D0])
        out_bits.append(bit); starts.append(t)
        scale = 0.97 * scale + 0.03 * scale * SCS[si]
        t += d0
        if verbose and n % 5120 == 0:
            print('  byte %04X t=%.2fs err %.2f scale %.4f' % (n // 10, t / 1e6, err[k], scale), flush=True)
    b = np.array(out_bits).reshape(-1, 10); mg = np.array(margins).reshape(-1, 10)
    vals = (b[:, 1:9] * (1 << np.arange(7, -1, -1))).sum(1)
    par_ok = b[:, 9] == (b[:, 1:9].sum(1) % 2 == 0)
    start_ok = b[:, 0] == 0
    return vals, par_ok, mg, np.array(starts).reshape(-1, 10)[:, 0], s, b


for it in range(3):
    vals, par_ok, mg, st, s_dec, b = decode(h, NBYTES)
    minm = mg[:, 1:].min(1)
    print('iter %d: parity errors %d of %d; ends %.2fs; weak bytes (min margin < 1): %d' % (
        it, (~par_ok).sum(), len(vals), (st[-1] + 10000) / 1e6, (minm < 1).sum()), flush=True)
    pickle.dump((vals, par_ok, mg, st), open(base + '_dec%d.pkl' % it, 'wb'))
    # refit channel on bytes with good parity and margins
    m2 = np.zeros(len(y), bool)
    for n in range(len(vals) - 1):
        if par_ok[n] and minm[n] > 2:
            m2[int(st[n] * us):int(st[n + 1] * us)] = True
    m2[int((t_start - 1.0e6) * us):int(t_start * us)] = True
    h, e = fit_h(s_dec, m2)
    print('  refit channel rms %.3f' % e, flush=True)

data = bytes(vals.astype(np.uint8).tolist())
open(base + '.bin', 'wb').write(data)
with open(base + '_hex.txt', 'w') as f:
    for o in range(0, len(data), 16):
        chunk = data[o:o + 16]
        f.write('%04X: %-48s %s\n' % (o, chunk.hex(' ').upper(),
                ''.join(chr(c) if 32 <= c < 127 else '.' for c in chunk)))
with open(base + '_lowconf.txt', 'w') as f:
    for n in range(len(vals)):
        if not par_ok[n] or minm[n] < 1:
            f.write('%04X %02X t=%.4fs parity=%s min_margin=%.2f\n' % (
                n, vals[n], st[n] / 1e6, 'ok' if par_ok[n] else 'BAD', minm[n]))
print('wrote', base + '.bin', len(data), 'bytes')
