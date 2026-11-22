"""Channel-model (MLSE-style) decoder core for COSMAC cassette formats.

Model: y = h * s + c. s is the ideal +/-1 square wave: every bit is one cycle,
high for duty*T then low; '1' = short cycle T1, '0' = long cycle T0.
h is fitted by least squares on the leader plus known bits, then bits are decided
one at a time by comparing all 2^LOOK continuations over the same time window.
"""
import numpy as np
from scipy.linalg import lstsq
from scipy.signal import butter, filtfilt, fftconvolve
from wavload import load

K1, K2 = 30, 110          # channel taps (samples): precursor / postcursor


class Tape:
    def __init__(self, fn, t0=None, t1=None, invert=False):
        a, self.sr = load(fn)
        if invert: a = -a
        if t0 is not None:
            a = a[int(t0 * self.sr):int(t1 * self.sr)]
            self.offset = t0
        else:
            self.offset = 0.0
        b, c = butter(1, 30 / (self.sr / 2), 'high')
        y = filtfilt(b, c, a)
        self.y = y / np.percentile(np.abs(y), 99)
        self.us = self.sr / 1e6

    # ---------- painting ----------
    def paint(self, s, t, bits, durs, duty):
        us = self.us
        for bt, d in zip(bits, durs):
            f = duty[bt]
            i0, i1, i2 = (int(round(x * us)) for x in (t, t + f * d, t + d))
            s[i0:i1] = 1; s[i1:i2] = -1; t += d
        return t

    def leader_anchors(self, ta, tb):
        y = self.y; sr = self.sr
        q = y[int(ta * sr):int(tb * sr)]
        u = np.nonzero((q[:-1] < 0) & (q[1:] >= 0))[0]
        U = (int(ta * sr) + u + q[u] / (q[u] - q[u + 1])) / self.us
        return U

    def paint_leader(self, s, U, t_end, T1, n=2000):
        """Paint leader cycles on anchors U, shifted so the cycle grid ends at t_end."""
        kk = np.searchsorted(U, t_end - 3 * T1)
        ref = U[max(0, kk - 40):kk]
        ph = np.angle(np.mean(np.exp(2j * np.pi * (ref - t_end) / T1))) / (2 * np.pi) * T1
        V = U - ph
        k1 = np.searchsorted(V, t_end - 0.5 * T1); k0 = max(0, k1 - n)
        for k in range(k0, k1):
            nxt = V[k + 1] if k + 1 < k1 else t_end
            d = nxt - V[k]
            if not (0.6 * T1 < d < 1.5 * T1): d = T1
            self.paint(s, V[k], [1], [d], {1: 0.5, 0: 0.5})
        return V[k0]

    # ---------- channel ----------
    def fit_h(self, s, mask, n=200000):
        y = self.y
        idx = np.nonzero(mask)[0]
        idx = idx[(idx > K2 + 1) & (idx < len(y) - K1 - 1)]
        sel = np.random.default_rng(0).choice(idx, size=min(len(idx), n), replace=False)
        X = np.stack([s[sel - k] for k in range(-K1, K2 + 1)] + [np.ones(len(sel))], 1)
        h, *_ = lstsq(X, y[sel])
        return h, np.sqrt(np.mean((X @ h - y[sel]) ** 2))

    @staticmethod
    def conv(S, h):
        out = fftconvolve(S, h[None, :-1], mode='full', axes=1)
        return out[:, K1:K1 + S.shape[1]] + h[-1]


def square(tg, rises, highs):
    k = (tg[None, :, None] >= rises[:, None, :]).sum(2) - 1
    M = highs.shape[1]
    inside = (k >= 0) & (k < M)
    kc = np.clip(k, 0, M - 1)
    r0 = np.take_along_axis(rises, kc, 1)
    hi = np.take_along_axis(highs, kc, 1)
    return np.where(inside, np.where(tg[None, :] - r0 < hi, 1.0, -1.0), 0.0)


def bootstrap(tp, U, t_guess, known_bits, T1, T0s, duty, span=400.0, step=5.0):
    """Search start time and 0-cycle length that best explain leader + known bits."""
    best = None
    for T0 in T0s:
        for ts in np.arange(t_guess - span, t_guess + span, step):
            s = np.zeros(int((ts + len(known_bits) * T0 * 1.2 + 5000) * tp.us) + K1 + 10)
            m = np.zeros_like(s, bool)
            a0 = tp.paint_leader(s, U, ts, T1, n=400)
            d = np.where(np.array(known_bits) == 1, T1, T0)
            te = tp.paint(s, ts, known_bits, d, duty)
            m[int(a0 * tp.us):int(te * tp.us)] = True
            ss = np.zeros_like(tp.y); ss[:len(s)] = s[:len(ss)]
            mm = np.zeros(len(tp.y), bool); mm[:len(m)] = m[:len(mm)]
            h, e = tp.fit_h(ss, mm, n=8000)
            if best is None or e < best[0]:
                best = (e, ts, T0)
    return best


def decode_bits(tp, h, U, t_start, T1, T0, duty, nbits, known_bits=(), LOOK=6, W=1400.0, PRE=200.0,
                SCS=(0.96, 0.98, 1.0, 1.02, 1.04), forced=None, verbose=True):
    """forced(n) -> None or 0/1 for bit index n (e.g. start bits)."""
    y = tp.y; us = tp.us
    s = np.zeros_like(y)
    tp.paint_leader(s, U, t_start, T1, n=3000)
    SCS = np.array(SCS)
    pats = np.array([[(p >> (LOOK - 1 - i)) & 1 for i in range(LOOK)] for p in range(2 ** LOOK)])
    wn = int((PRE + W) * us)
    t = t_start; scale = 1.0
    bits, margins, starts = [], [], []
    hi_f = np.array([duty[0], duty[1]])
    while len(bits) < nbits:
        n = len(bits)
        cand = pats.copy()
        for j in range(LOOK):
            f = known_bits[n + j] if n + j < len(known_bits) else (forced(n + j) if forced else None)
            if f is not None: cand[:, j] = f
        cand = np.unique(cand, axis=0); C = len(cand)
        base = np.where(cand == 1, T1, T0) * scale
        D = np.repeat(base[None], len(SCS), 0); D[:, :, 0] *= SCS[:, None]
        D = D.reshape(-1, LOOK)
        HF = np.tile(hi_f[cand], (len(SCS), 1)) * D
        r = np.concatenate([np.full((len(D), 1), t), t + np.cumsum(D, 1)], 1)
        if r[:, -1].min() < t + W:  # extend with ones if window not covered
            pass
        g0 = int(round((t - PRE) * us)) - K2
        if g0 + K2 + wn + K1 > len(y): break
        tg = np.arange(g0, g0 + K2 + wn + K1) / us
        S = square(tg, r, HF)
        past = tg < t
        S[:, past] = s[g0:g0 + len(tg)][past]
        Y = tp.conv(S, h)[:, K2:K2 + wn]
        err = ((Y - y[g0 + K2:g0 + K2 + wn]) ** 2).sum(1)
        k = int(np.argmin(err)); si, ci = divmod(k, C)
        bit = int(cand[ci, 0])
        first = np.tile(cand[:, 0], len(SCS))
        alt = err[first != bit]
        margins.append(alt.min() - err[k] if len(alt) else np.inf)
        d0 = D[k, 0]
        tp.paint(s, t, [bit], [d0], duty)
        bits.append(bit); starts.append(t)
        scale = min(max(0.98 * scale + 0.02 * scale * SCS[si], 0.97), 1.03)
        t += d0
        if verbose and n % 4000 == 0:
            print('  bit %6d t=%.2fs scale %.4f err %.2f' % (n, (t / 1e6) + tp.offset, scale, err[k]), flush=True)
    return np.array(bits), np.array(margins), np.array(starts), s
