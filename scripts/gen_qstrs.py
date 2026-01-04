
def compute_hash(s, bits=16):
    mask = (1 << bits) - 1
    h = 5381
    for c in s:
        h = ((h << 5) + h) ^ ord(c)
    h &= mask
    if h == 0:
        h = 1
    return h

qstrs = [
    "filter", "iterable", "enumerate", "fromkeys", "StopAsyncIteration",
    "__file__", "__path__", "array", "__aiter__", "__anext__",
    "default", "property", "reversed", "slice", "delattr", "help", "max", "min",
    "sys", "itertools", "heapq", "collections", "uasyncio", "hashlib", "binascii",
    "json", "re", "struct", "array", "math", "cmath", "gc", "micropython",
    "cardputer", "clear", "set_pixel", "fill_rect", "poll_key", "screen_width", "screen_height", "present", "i2s_write"
]

# Special cases for strings that are not valid identifiers in the QDEF macro but used in code
# MicroPython often defines them like QDEF(MP_QSTR__0x0a_, ...)
# But the actual string is "\n"
special = {
    "_percent__hash_x": "%#x",
    "_percent__hash_o": "%#o",
    "_0x0a_": "\n",
    "_space_": " ",
}

for q in qstrs:
    print(f'QDEF1(MP_QSTR_{q}, {compute_hash(q)}, {len(q)}, "{q}")')

for id, s in special.items():
    print(f'QDEF1(MP_QSTR_{id}, {compute_hash(s)}, {len(s)}, "{s.encode("unicode_escape").decode()}")')
