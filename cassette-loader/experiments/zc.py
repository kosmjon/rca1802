import numpy as np, wave, sys, collections
from scipy.signal import butter, filtfilt

frac = float(sys.argv[1]) if len(sys.argv) > 1 else 0.06
lp = float(sys.argv[2]) if len(sys.argv) > 2 else 6000
w = wave.open('TinyBasic.wav'); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
b, c = butter(2, [100 / (sr / 2), lp / (sr / 2)], 'band')
x = filtfilt(b, c, a)
thr = frac * np.percentile(np.abs(x), 99)


def edges(x, thr):
    st = x[0] > 0; R = []; F = []
    hi = x > thr; lo = x < -thr
    idx = np.nonzero(hi | lo)[0]
    for i in idx:
        if not st and hi[i]: st = True; R.append(i)
        elif st and lo[i]: st = False; F.append(i)
    return np.array(R), np.array(F)


R, F = edges(x, thr)
for E in (R, F):
    E = E[E > 8.27 * sr]; p = np.diff(E) / sr * 1e6
    h, bb = np.histogram(p, bins=np.arange(0, 2200, 100))
    print([(int(k), int(v)) for k, v in zip(bb, h) if v > 10])
E = R[R > 8.265 * sr]; p = np.diff(E) / sr * 1e6
print(np.round(p[:120]).astype(int))
