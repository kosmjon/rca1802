"""Waveform-level decoder for Netronics Elf II cassette audio.

Model: y = h * s + noise, where s is the ideal +/-1 Q-line square wave and h is
a linear channel response (recorder + tape + sound card) fitted by least squares.
Each byte frame (start 0, 8 data bits MSB first, parity = 1 if even ones) is decided
by synthesising s for all 256 byte values (and small timing/speed variations),
passing it through h, and choosing the candidate with the smallest squared error
against the recorded waveform. Previously decided frames are kept in s so their
inter-symbol interference is modelled.

Usage: python mlse.py TinyBasic.wav
"""
import numpy as np, sys, pickle
from scipy.linalg import lstsq
from scipy.signal import fftconvolve
import framedec_lib as fl

WAV = sys.argv[1] if len(sys.argv) > 1 else 'TinyBasic.wav'
KNOWN = bytes.fromhex('C0 80 00 C0 F0 00 D0 00 00 00 00 00 A0 9A')
NBYTES = 0x0FFF
y = fl.load_y(WAV)
sr = fl.sr
us = sr / 1e6  # samples per microsecond

# ---- timing model (microseconds); refined below from data ----
T1, T0, TS = 430.0, 1215.0, 1285.0
D1, D0 = 0.43, 0.47  # high-time fraction of a cycle
K1, K2 = 24, 120     # channel taps: precursor / postcursor (samples)


def bit_durs(bits, t1, t0, ts):
    d = np.where(np.asarray(bits) == 1, t1, t0).astype(float)
    d[..., 0] = ts
    return d


def paint(s, t, bits, durs):
    """Paint Q square wave for one frame starting at time t (us) into s."""
    for b, d in zip(bits, durs):
        a = int(round(t * us)); f = int(round((t + (D1 if b else D0) * d) * us))
        e = int(round((t + d) * us))
        s[a:f] = 1; s[f:e] = -1
        t += d
    return t


def fit_h(s, mask):
    idx = np.nonzero(mask)[0]
    idx = idx[(idx > K2) & (idx < len(y) - K1)]
    rng = np.random.default_rng(0)
    sel = rng.choice(idx, size=min(len(idx), 300000), replace=False)
    X = np.stack([s[sel - k] for k in range(-K1, K2 + 1)] + [np.ones(len(sel))], 1)
    h, *_ = lstsq(X, y[sel])
    err = np.sqrt(np.mean((X @ h - y[sel]) ** 2))
    return h, err


def predict(S, h):
    """S: (..., N) candidate square waves -> predicted y, same alignment."""
    k = h[:-1]  # taps for lags -K1..K2
    # y[n] = sum_j k[j] s[n - (j - K1)]  -> convolution with k, shifted by K1
    out = fftconvolve(S, k[None, :] if S.ndim == 2 else k, mode='full', axes=-1)
    return out[..., K1:K1 + S.shape[-1]] + h[-1]


# ---- bootstrap: edge-based alignment of leader + known bytes ----
z = np.load('z2.npy').astype(float)
R = fl.rises(z, 0.3) / sr * 1e6
P = np.diff(R)
i = np.searchsorted(R, 8.0e6)
while P[i] < 700: i += 1
t_data = R[i]
print('first long cycle at %.4fs' % (t_data / 1e6))

s = np.zeros_like(y); mask = np.zeros(len(y), bool)
# leader: last 1.5 s of short cycles, use observed rises
j0 = np.searchsorted(R, t_data - 1.5e6)
for j in range(j0, i):
    a, e = int(R[j] * us), int(R[j + 1] * us)
    f = int((R[j] + D1 * (R[j + 1] - R[j])) * us)
    s[a:f] = 1; s[f:e] = -1; mask[a:e] = True

# known bytes: search start offset/scale that best fit edges, then paint
best = None
for dt in np.arange(-600, 601, 20):
    for sc in np.linspace(0.95, 1.05, 11):
        t = t_data + dt; c = 0
        for v in KNOWN:
            d = bit_durs(fl.BITS[v], T1, T0, TS) * sc
            e = t + np.cumsum(d)
            jj = np.searchsorted(R, e)
            dd = np.minimum(np.abs(R[np.clip(jj - 1, 0, len(R) - 1)] - e),
                            np.abs(R[np.clip(jj, 0, len(R) - 1)] - e))
            c += np.minimum(dd, 250).sum(); t = e[-1]
        if best is None or c < best[0]:
            best = (c, dt, sc)
print('known-bytes edge fit: cost %.0f dt %d scale %.3f' % best)
_, dt, sc = best
t = t_data + dt
t_known = t
for v in KNOWN:
    d = bit_durs(fl.BITS[v], T1, T0, TS) * sc
    a = int(t * us)
    t = paint(s, t, fl.BITS[v], d)
    mask[a:int(t * us)] = True
h, err = fit_h(s, mask)
print('initial channel fit rms %.3f' % err)


# ---- sequential decoding ----
def decode_all(h, t_start, scale0, first_vals, nbytes):
    s = np.zeros_like(y)
    # paint leader (for ISI) as short cycles before t_start
    tt = t_start - 2000 * T1 * scale0
    while tt + T1 * scale0 <= t_start:
        a = int(tt * us); f = int((tt + D1 * T1 * scale0) * us); e = int((tt + T1 * scale0) * us)
        s[a:f] = 1; s[f:e] = -1; tt += T1 * scale0
    t = t_start; scale = scale0
    vals, info, starts = [], [], []
    allbits = fl.BITS  # (256,10)
    W_pre = 300.0  # us before frame start included in error window
    for n in range(nbytes):
        best = None
        for sc in scale * np.array([0.985, 0.995, 1.0, 1.005, 1.015]):
            for off in (-40.0, -20.0, 0.0, 20.0, 40.0):
                t0 = t + off
                d = bit_durs(allbits, T1, T0, TS) * sc  # (256,10)
                tend = t0 + d.sum(1)
                a = int((t0 - W_pre) * us)
                b = int((tend.max() + TS * sc) * us) + 4
                N = b - a
                S = np.tile(s[a - K2:b + K1], (256, 1)).astype(float)
                # clear future, paint candidates + following start-bit high/low
                S[:, K2 + int((t0 - W_pre) * us) - a:] = 0
                base = a - K2
                for v in range(256):
                    tv = t0
                    for k in range(10):
                        bb = allbits[v, k]; dk = d[v, k]
                        p = int(round(tv * us)) - base; f = int(round((tv + (D1 if bb else D0) * dk) * us)) - base
                        e = int(round((tv + dk) * us)) - base
                        S[v, p:f] = 1; S[v, f:e] = -1; tv += dk
                    # next start bit (always 0)
                    p = int(round(tv * us)) - base; f = int(round((tv + D0 * TS * sc) * us)) - base
                    e = min(int(round((tv + TS * sc) * us)) - base, S.shape[1])
                    S[v, p:f] = 1; S[v, f:e] = -1
                Y = predict(S, h)[:, K2:K2 + N]
                ye = y[a:b]
                # error only up to each candidate's own end + 150us
                lim = ((tend + 150) * us).astype(int) - a
                cols = np.arange(N)[None, :]
                wmask = cols < lim[:, None]
                err = (((Y - ye) ** 2) * wmask).sum(1) / wmask.sum(1)
                if first_vals is not None and n < len(first_vals):
                    err = np.where(np.arange(256) == first_vals[n], err, np.inf)
                k = int(np.argmin(err))
                if best is None or err[k] < best[0]:
                    e2 = np.partition(err, 1)[1]
                    best = (err[k], k, sc, t0, tend[k], e2)
        e1, v, sc, t0, tend, e2 = best
        paint(s, t0, allbits[v], bit_durs(allbits[v], T1, T0, TS) * sc)
        vals.append(v); info.append((t0, e1, e2, sc)); starts.append(t0)
        scale = 0.8 * scale + 0.2 * sc
        t = tend
        if n % 256 == 0:
            print('  byte %04X t=%.2fs err %.3f scale %.3f' % (n, t0 / 1e6, e1, sc), flush=True)
    return np.array(vals), info, s


if __name__ == '__main__':
    for it in range(2):
        vals, info, s_dec = decode_all(h, t_known, sc, KNOWN[:2] if it == 0 else None, NBYTES)
        e1 = np.array([x[1] for x in info]); e2 = np.array([x[2] for x in info])
        print('iter %d: median err %.3f, low-margin frames (ratio<1.3): %d' % (
            it, np.median(e1), (e2 / e1 < 1.3).sum()))
        print('first bytes:', bytes(vals[:14]).hex(' ').upper(),
              'match known:', bytes(vals[:14]) == KNOWN)
        pickle.dump((vals, info), open('mlse_out_%d.pkl' % it, 'wb'))
        # re-fit channel on confident frames
        m2 = np.zeros(len(y), bool)
        for (t0, a1, a2, scx), v in zip(info, vals):
            if a2 / a1 > 1.5:
                d = bit_durs(fl.BITS[v], T1, T0, TS) * scx
                m2[int(t0 * us):int((t0 + d.sum()) * us)] = True
        h, err = fit_h(s_dec, m2 | mask)
        print('refit channel rms %.3f' % err)
    open('TinyBasic.bin', 'wb').write(bytes(vals))
