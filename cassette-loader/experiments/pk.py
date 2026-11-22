import numpy as np, wave, sys
from scipy.signal import butter, filtfilt, find_peaks

pr = float(sys.argv[1]) if len(sys.argv) > 1 else 0.15
w = wave.open('TinyBasic.wav'); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
b, c = butter(2, [80 / (sr / 2), 6000 / (sr / 2)], 'band')
x = filtfilt(b, c, a)
P = np.percentile(np.abs(x), 99) * pr
mx, _ = find_peaks(x, prominence=P)
mn, _ = find_peaks(-x, prominence=P)
ev = sorted([(i, 1) for i in mx] + [(i, 0) for i in mn])
# enforce alternation: keep the more extreme of consecutive same-type
alt = []
for e in ev:
    if alt and alt[-1][1] == e[1]:
        if (x[e[0]] > x[alt[-1][0]]) == (e[1] == 1):
            alt[-1] = e
        continue
    alt.append(e)
t = np.array([e[0] for e in alt]) / sr * 1e6
pol = np.array([e[1] for e in alt])
D = np.diff(t)
h, bb = np.histogram(D[t[:-1] > 8.27e6], bins=np.arange(0, 1400, 50))
print([(int(k), int(v)) for k, v in zip(bb, h) if v > 10])
i0 = np.nonzero(t > 8.265e6)[0][0]
print(' '.join('%s%d' % ('^' if pol[j] else 'v', D[j]) for j in range(i0, i0 + 150)))
np.save('extrema.npy', np.c_[t, pol])
