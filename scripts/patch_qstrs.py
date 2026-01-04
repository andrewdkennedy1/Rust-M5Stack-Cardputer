
header_path = 'components/micropython_embed/embedded/genhdr/qstrdefs.generated.h'
new_qstrs = [
    'QDEF1(MP_QSTR_filter, 48677, 6, "filter")',
    'QDEF1(MP_QSTR_iterable, 37413, 8, "iterable")',
    'QDEF1(MP_QSTR_enumerate, 47729, 9, "enumerate")',
    'QDEF1(MP_QSTR_fromkeys, 48439, 8, "fromkeys")',
    'QDEF1(MP_QSTR_StopAsyncIteration, 61676, 18, "StopAsyncIteration")',
    'QDEF1(MP_QSTR___file__, 21507, 8, "__file__")',
    'QDEF1(MP_QSTR___path__, 9160, 8, "__path__")',
    'QDEF1(MP_QSTR_array, 29308, 5, "array")',
    'QDEF1(MP_QSTR___aiter__, 11086, 9, "__aiter__")',
    'QDEF1(MP_QSTR___anext__, 46211, 9, "__anext__")',
    'QDEF1(MP_QSTR_default, 32206, 7, "default")',
    'QDEF1(MP_QSTR_property, 10690, 8, "property")',
    'QDEF1(MP_QSTR_reversed, 28321, 8, "reversed")',
    'QDEF1(MP_QSTR_slice, 62645, 5, "slice")',
    'QDEF1(MP_QSTR_delattr, 51419, 7, "delattr")',
    'QDEF1(MP_QSTR_help, 23700, 4, "help")',
    'QDEF1(MP_QSTR_max, 17329, 3, "max")',
    'QDEF1(MP_QSTR_min, 17071, 3, "min")',
    'QDEF1(MP_QSTR_sys, 36540, 3, "sys")',
    'QDEF1(MP_QSTR_gc, 28257, 2, "gc")',
    'QDEF1(MP_QSTR_micropython, 31755, 11, "micropython")',
    'QDEF1(MP_QSTR__percent__hash_x, 6779, 3, "%#x")',
    'QDEF1(MP_QSTR__percent__hash_o, 6764, 3, "%#o")',
]

with open(header_path, 'a') as f:
    f.write('\n// Patched by AI to fix missing definitions\n')
    for q in new_qstrs:
        f.write(q + '\n')
print("Patched " + header_path)
