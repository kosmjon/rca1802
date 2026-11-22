"""Fit a linear equalizer g so that g*y ~= s (ideal Q square wave) on known regions."""
import numpy as np, wave
from scipy.signal import butter, filtfilt
from scipy.linalg import lstsq

w = wave.open('TinyBasic.wav'); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
b, c = butter(1, 40 / (sr / 2), 'high')
y = filtfilt(b, c, a)
y /= np.percentile(np.abs(y), 99)

# Schmitt rising edges at 0.5
thr = 0.5
st = y[0] > 0; R = []
hi = y > thr; lo = y < -thr
for i in np.nonzero(hi | lo)[0]:
    if not st and hi[i]: st = True; R.append(i)
    elif st and lo[i]: st = False
R = np.array(R)
P = np.diff(R) / sr * 1e6

# known segments: runs of consecutive clean periods
s = np.zeros_like(y); mask = np.zeros(len(y), bool)
def clean(p): return (380 < p < 460) or (1170 < p < 1290)
run = []
segs = 0
for k in range(len(P)):
    if clean(P[k]):
        run.append(k)
        continue
    if len(run) >= 6:
        for j in run:
            r0, r1 = R[j], R[j + 1]
            duty = 0.43 if P[j] < 700 else 0.47
            f = int(r0 + duty * (r1 - r0))
            s[r0:f] = 1; s[f:r1] = -1
            mask[r0:r1] = True
        segs += 1
    run = []
print('segments', segs, 'known secs', mask.sum() / sr)

# Fit FIR equalizer, taps from -L1..+L2 relative (non-causal)
L1, L2 = 60, 60
N = L1 + L2 + 1
idx = np.nonzero(mask)[0]
idx = idx[(idx > L2) & (idx < len(y) - L1)]
# balance: subsample leader so 0-runs carry weight
rng = np.random.default_rng(0)
sel = rng.choice(idx, size=min(len(idx), 200000), replace=False)
X = np.stack([y[sel + k] for k in range(-L2, L1 + 1)], 1)
g, *_ = lstsq(X, s[sel])
np.save('g.npy', g)
from scipy.signal import fftconvolve
z = fftconvolve(y, g[::-1], 'same')  # == correlate(y, g, 'same') for odd N
assert abs(z[sel[0]] - X[0] @ g) < 1e-6, (z[sel[0]], X[0] @ g)
# 'same' alignment: check residual
pred = X @ g
print('fit rms err', np.sqrt(np.mean((pred - s[sel]) ** 2)))
np.save('z.npy', z.astype(np.float32))
