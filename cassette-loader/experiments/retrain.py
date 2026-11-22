"""Decision-directed equalizer retraining from decoded frames."""
import numpy as np, wave, sys
from scipy.signal import butter, filtfilt, fftconvolve
from scipy.linalg import lstsq
import framedec_lib as fl

zin, zout, t_start = sys.argv[1], sys.argv[2], float(sys.argv[3])
L = int(sys.argv[4]) if len(sys.argv) > 4 else 72
maxcost = float(sys.argv[5]) if len(sys.argv) > 5 else 300
sr = 48000
y = fl.load_y()
z = np.load(zin).astype(float)
frames = fl.decode(z, 0.5, t_start)
s = np.zeros_like(y); mask = np.zeros(len(y), bool)
used = 0
for fr in frames:
    if fr['cost'] > maxcost: continue
    used += 1
    r = (np.asarray(fr['rises']) * sr / 1e6).astype(int)
    for k in range(10):
        r0, r1 = r[k], r[k + 1]
        duty = 0.43 if fr['bits'][k] else 0.47
        f = int(r0 + duty * (r1 - r0))
        s[r0:f] = 1; s[f:r1] = -1; mask[r0:r1] = True
print('frames used', used, 'of', len(frames))
idx = np.nonzero(mask)[0]
idx = idx[(idx > L) & (idx < len(y) - L)]
rng = np.random.default_rng(1)
sel = rng.choice(idx, size=min(len(idx), 400000), replace=False)
X = np.stack([y[sel + k] for k in range(-L, L + 1)], 1)
g, *_ = lstsq(X, s[sel])
print('fit rms err %.3f' % np.sqrt(np.mean((X @ g - s[sel]) ** 2)))
z2 = fftconvolve(y, g[::-1], 'same')
np.save(zout, z2.astype(np.float32))
