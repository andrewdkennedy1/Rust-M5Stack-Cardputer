MicroPython embed package for Cardputer.

Regenerate the embedded sources:

```
cd components/micropython_embed
MICROPY_QSTR_NO_THREADS=1 SRC_QSTR=$(pwd)/cardputer_module.c \
  make -B -f ../micropython/ports/embed/embed.mk \
  MICROPYTHON_TOP=../micropython PACKAGE_DIR=embedded
```

Notes:
- `MICROPY_QSTR_NO_THREADS=1` avoids sandbox issues with Python semaphores.
- `SRC_QSTR` ensures `cardputer_module.c` is scanned for qstrs/modules.
