import wave, numpy as np


def load(fn):
    """Return (samples as float in [-1,1], sample rate), first channel only."""
    w = wave.open(fn)
    sr, ch, sw = w.getframerate(), w.getnchannels(), w.getsampwidth()
    raw = np.frombuffer(w.readframes(w.getnframes()), np.uint8)
    if sw == 3:
        b = raw.reshape(-1, 3).astype(np.int32)
        x = (b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16))
        x = np.where(x >= 1 << 23, x - (1 << 24), x).astype(float) / (1 << 23)
    elif sw == 2:
        x = raw.view(np.int16).astype(float) / 32768
    elif sw == 4:
        x = raw.view(np.int32).astype(float) / 2 ** 31
    else:
        x = (raw.astype(float) - 128) / 128
    return x.reshape(-1, ch)[:, 0], sr
