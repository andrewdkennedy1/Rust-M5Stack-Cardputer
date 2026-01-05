
import os

header_path = 'components/micropython_embed/embedded/genhdr/qstrdefs.generated.h'
if not os.path.exists(header_path):
    print("File not found")
    exit(1)

with open(header_path, 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if "Patched by AI" in line:
        continue
    # Filter out our specific QSTRs if they already exist in the file
    # This is to avoid duplicates if the build system actually has them elsewhere
    is_duplicate = False
    for q in ["filter", "iterable", "enumerate", "fromkeys", "StopAsyncIteration", "__file__", "__path__", "array", "__aiter__", "__anext__", "default", "property", "reversed", "slice", "delattr", "help", "max", "min", "sys", "gc", "micropython", "_percent__hash_x", "_percent__hash_o"]:
        if f"MP_QSTR_{q}," in line or f"MP_QSTR_{q} " in line:
             is_duplicate = True
             break
    if not is_duplicate:
        new_lines.append(line)

new_qstrs = [
    'QDEF0(MP_QSTR_filter, 48677, 6, "filter")',
    'QDEF0(MP_QSTR_iterable, 37413, 8, "iterable")',
    'QDEF0(MP_QSTR_enumerate, 47729, 9, "enumerate")',
    'QDEF0(MP_QSTR_fromkeys, 48439, 8, "fromkeys")',
    'QDEF0(MP_QSTR_StopAsyncIteration, 61676, 18, "StopAsyncIteration")',
    'QDEF0(MP_QSTR___file__, 21507, 8, "__file__")',
    'QDEF0(MP_QSTR___path__, 9160, 8, "__path__")',
    'QDEF0(MP_QSTR_array, 29308, 5, "array")',
    'QDEF0(MP_QSTR___aiter__, 11086, 9, "__aiter__")',
    'QDEF0(MP_QSTR___anext__, 46211, 9, "__anext__")',
    'QDEF0(MP_QSTR_default, 32206, 7, "default")',
    'QDEF0(MP_QSTR_property, 10690, 8, "property")',
    'QDEF0(MP_QSTR_reversed, 28321, 8, "reversed")',
    'QDEF0(MP_QSTR_slice, 62645, 5, "slice")',
    'QDEF0(MP_QSTR_delattr, 51419, 7, "delattr")',
    'QDEF0(MP_QSTR_help, 23700, 4, "help")',
    'QDEF0(MP_QSTR_max, 17329, 3, "max")',
    'QDEF0(MP_QSTR_min, 17071, 3, "min")',
    'QDEF0(MP_QSTR_sys, 36540, 3, "sys")',
    'QDEF0(MP_QSTR_gc, 28257, 2, "gc")',
    'QDEF0(MP_QSTR_micropython, 31755, 11, "micropython")',
    'QDEF0(MP_QSTR__percent__hash_x, 6779, 3, "%#x")',
    'QDEF0(MP_QSTR__percent__hash_o, 6764, 3, "%#o")',
]

with open(header_path, 'w') as f:
    f.writelines(new_lines)
    f.write('\n// Patched by AI to fix missing definitions\n')
    for q in new_qstrs:
        f.write(q + '\n')
print("Cleaned and Patched " + header_path)
