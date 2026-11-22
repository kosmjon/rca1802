import numpy as np, sys
sr = 48000
z = np.load(sys.argv[1] if len(sys.argv) > 1 else 'z.npy').astype(float)


def rises(z, thr):
    st = z[0] > 0; R = []
    hi = z > thr; lo = z < -thr
    for i in np.nonzero(hi | lo)[0]:
        if not st and hi[i]: st = True; R.append(i)
        elif st and lo[i]: st = False
    return np.array(R)


def run(thr, cut=800, show=False):
    R = rises(z, thr)
    P = np.diff(R) / sr * 1e6
    # leader end
    k = np.nonzero(R > 8.0 * sr)[0][0]
    while P[k] < cut: k += 1
    bits = (P[k:] < cut).astype(int)
    nf = len(bits) // 10
    ok = 0; st0 = 0; vals = []
    for f in range(nf):
        fr = bits[f * 10:f * 10 + 10]
        v = int(''.join(map(str, fr[1:9])), 2)
        par = 1 if bin(v).count('1') % 2 == 0 else 0
        ok += fr[9] == par; st0 += fr[0] == 0; vals.append(v)
        if show and f < 40: print(''.join(map(str, fr)), '%02X' % v, fr[9] == par)
    return nf, ok, st0, vals


if __name__ == '__main__':
    for thr in (0.1, 0.2, 0.3, 0.4, 0.5, 0.6):
        nf, ok, st0, _ = run(thr)
        print(thr, nf, ok, st0)
