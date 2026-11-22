import numpy as np, wave, sys
from scipy.signal import butter, filtfilt

w = wave.open('TinyBasic.wav'); sr = w.getframerate()
a = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(float)
b, c = butter(2, [150 / (sr / 2), 4000 / (sr / 2)], 'band')
x = filtfilt(b, c, a)
pk = np.percentile(np.abs(x), 99)


def halves(x, frac):
    """Zero crossings, merging half-cycles whose excursion is below frac*pk."""
    s = np.signbit(x)
    zc = np.nonzero(s[1:] != s[:-1])[0]
    # interpolate
    t = zc + x[zc] / (x[zc] - x[zc + 1])
    segs = []
    for i in range(len(t) - 1):
        seg = x[zc[i] + 1:zc[i + 1] + 1]
        ext = seg.max() if seg.mean() > 0 else -seg.min()
        segs.append([t[i], t[i + 1], 1 if seg.mean() > 0 else 0, ext])
    # merge weak segments into neighbours: drop weak seg and the next crossing
    out = []
    for sg in segs:
        if out and (sg[3] < frac * pk or sg[2] == out[-1][2]):
            # weak or same polarity -> absorb
            out[-1][1] = sg[1]
            continue
        out.append(sg)
    # second pass: merge same-polarity neighbours created by absorption
    res = []
    for sg in out:
        if res and sg[2] == res[-1][2]:
            res[-1][1] = sg[1]
        else:
            res.append(sg)
    return res


if __name__ == '__main__':
    frac = float(sys.argv[1]) if len(sys.argv) > 1 else 0.4
    H = halves(x, frac)
    d = np.array([(h[1] - h[0]) / sr * 1e6 for h in H])
    st = np.array([h[0] / sr for h in H])
    m = st > 8.27
    hh, bb = np.histogram(d[m], bins=np.arange(0, 2000, 50))
    for k, v in zip(bb, hh):
        if v > 20: print(k, v)
    i = np.nonzero(st > 8.265)[0][0]
    print(' '.join('%s%d' % ('H' if H[j][2] else 'L', round(d[j])) for j in range(i, i + 120)))
