# Stub for machine module
class I2S:
    TX = 0
    MONO = 0
    def __init__(self, id, **kwargs):
        pass
    def write(self, buf):
        import cardputer
        if hasattr(cardputer, "i2s_write"):
            cardputer.i2s_write(buf)
    def deinit(self): pass

class Pin:
    def __init__(self, *args, **kwargs): pass

def freq(f=None):
    return 240000000
