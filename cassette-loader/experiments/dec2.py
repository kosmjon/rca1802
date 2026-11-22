"""Iterative decision-directed equalizer + decoder for Elf II cassette WAV."""
import numpy as np, wave, sys, collections
from scipy.signal import butter, filtfilt, fftconvolve
from scipy.linalg import lstsq

FN = sys.argv[1] if len(sys.argv) > 1 else 'TinyBasic.wav'
w = wave.open(FN); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
if w.getnchannels() > 1:
    a = a[::w.getnchannels()]
b, c = butter(1, 40 / (sr / 2), 'high')
y = filtfilt(b, c, a)
y /= np.percentile(np.abs(y), 99)
L = 60


def rises(z, thr):
    st = z[0] > 0; R = []; F = []
    hi = z > thr; lo = z < -thr
    for i in np.nonzero(hi | lo)[0]:
        if not st and hi[i]: st = True; R.append(i)
        elif st and lo[i]: st = False; F.append(i)
    return np.array(R), np.array(F)


def decode(z, thr=0.3, verbose=False):
    R, F = rises(z, thr)
    P = np.diff(R) / sr * 1e6
    # fall time within each cycle (for duty / training)
    fi = np.searchsorted(F, R[:-1])
    fall = np.where(fi < len(F), F[np.minimum(fi, len(F) - 1)], R[1:])
    # leader: find long run of short cycles
    short = P < 800
    best = None
    k = 0
    frames = []
    while k < len(P):
        j = k
        while j < len(P) and short[j]: j += 1
        if j - k >= 500:
            # data begins at cycle j (start bit)
            pos = j
            while pos + 10 <= len(P):
                bits = [0 if P[q] >= 800 else 1 for q in range(pos, pos + 10)]
                if bits[0] != 0 or P[pos] > 3000:
                    break
                val = 0
                for bt in bits[1:9]: val = (val << 1) | bt
                ok = bits[9] == (1 if bin(val).count('1') % 2 == 0 else 0)
                frames.append((pos, val, ok))
                pos += 10
            if verbose:
                print('leader %d cycles ending %.3fs; %d frames, stopped at %.3fs (P=%s)' % (
                    j - k, R[j] / sr, len(frames), R[min(pos, len(R) - 1)] / sr,
                    np.round(P[pos:pos + 12]).astype(int) if pos < len(P) else None))
            k = pos
            continue
        k = j + 1
    return R, F, P, fall, frames


def train(z_bits_cycles, R, fall):
    s = np.zeros_like(y); mask = np.zeros(len(y), bool)
    for q in z_bits_cycles:
        r0, f, r1 = R[q], fall[q], R[q + 1]
        if not (r0 < f < r1): continue
        s[r0:f] = 1; s[f:r1] = -1; mask[r0:r1] = True
    idx = np.nonzero(mask)[0]
    idx = idx[(idx > L) & (idx < len(y) - L)]
    rng = np.random.default_rng(0)
    sel = rng.choice(idx, size=min(len(idx), 300000), replace=False)
    X = np.stack([y[sel + k] for k in range(-L, L + 1)], 1)
    g, *_ = lstsq(X, s[sel])
    err = np.sqrt(np.mean((X @ g - s[sel]) ** 2))
    return g, err


z = y.copy()
g = np.load('g.npy') if len(sys.argv) <= 2 else None
if g is not None:
    z = fftconvolve(y, g[::-1], 'same')
for it in range(6):
    R, F, P, fall, frames = decode(z, verbose=True)
    nok = sum(f[2] for f in frames)
    print('iter', it, 'frames', len(frames), 'parity ok', nok)
    # train on cycles from parity-good frames + leader
    cyc = []
    for pos, val, ok in frames:
        if ok: cyc.extend(range(pos, pos + 10))
    lead = [q for q in range(len(P)) if P[q] < 500 and R[q] < frames[0][0] and False]
    if frames:
        p0 = frames[0][0]
        cyc.extend(range(max(0, p0 - 3000), p0))
    g, err = train(cyc, R, fall)
    print('  train cycles', len(cyc), 'err %.3f' % err)
    z = fftconvolve(y, g[::-1], 'same')
np.save('z.npy', z.astype(np.float32)); np.save('g2.npy', g)
