from machine import I2S, Pin, freq
import time, gc, math, struct
from lib.display import Display
from lib.userinput import UserInput
from lib.hydra.config import Config

# ───────────── System Init ─────────────
SRATE = 44100
CHUNK_SIZE = 512 # Smaller chunks for synthesis responsiveness

# ───────────── Peripherals ─────────────
tft = Display()
cfg = Config()
kb  = UserInput()
W, H = tft.width, tft.height

# I2S global
i2s = None

# ──────────── Note Mapping (C4 base) ────────────
# Frequency = 440 * 2^((n-69)/12)
# C4 is MIDI 60
NOTES = {
    "A": 60, "W": 61, "S": 62, "E": 63, "D": 64,
    "F": 65, "T": 66, "G": 67, "Y": 68, "H": 69,
    "U": 70, "J": 71, "K": 72
}

LETTER_POS = {
    "A": ("white", 0), "W": ("black", 0),
    "S": ("white", 1), "E": ("black", 1),
    "D": ("white", 2),
    "F": ("white", 3), "T": ("black", 3),
    "G": ("white", 4), "Y": ("black", 4),
    "H": ("white", 5), "U": ("black", 5),
    "J": ("white", 6),
    "K": ("white", 7)
}

# state
octave = 0
active_note_key = None
active_freq = 0
phase = 0
buffer = bytearray(CHUNK_SIZE * 2) # 16-bit mono

# UI Geometry
white_w  = W // 8
white_h  = 60
black_w  = white_w // 2
black_h  = 40
highlight_margin         = 4
white_highlight_w        = white_w - 2 * highlight_margin
white_highlight_y_offset = black_h
white_highlight_h        = white_h - black_h
piano_y     = H - white_h

last_octave = 0
last_key = None

def get_freq(midi_note, octave_offset):
    return 440.0 * math.pow(2.0, ((midi_note + octave_offset * 12) - 69.0) / 12.0)

def draw_static_ui():
    tft.fill(cfg.palette[2])
    for i in range(8):
        x = i * white_w
        tft.fill_rect(x, piano_y, white_w, white_h, cfg.palette[8])
        tft.rect(x, piano_y, white_w, white_h, cfg.palette[7])
    for i in [0,1,3,4,5]:
        x = i * white_w + white_w - black_w // 2
        tft.fill_rect(x, piano_y, black_w, black_h, cfg.palette[0])
        tft.rect(x, piano_y, black_w, black_h, cfg.palette[7])
    tft.show()

def update_display():
    global last_key, last_octave
    if octave != last_octave:
        tft.fill_rect(0, piano_y-28, W, 28, cfg.palette[2])
        last_octave = octave

    if last_key and last_key != active_note_key:
        kind, idx = LETTER_POS[last_key]
        if kind == 'white':
            x = idx * white_w + highlight_margin
            y = piano_y + white_highlight_y_offset
            tft.fill_rect(x, y, white_highlight_w, white_highlight_h, cfg.palette[8])
        else:
            x = idx * white_w + white_w - black_w // 2
            tft.fill_rect(x, piano_y, black_w, black_h, cfg.palette[0])
            tft.rect(x, piano_y, black_w, black_h, cfg.palette[7])

    if active_note_key:
        kind, idx = LETTER_POS[active_note_key]
        if kind == 'white':
            x = idx * white_w + highlight_margin
            y = piano_y + white_highlight_y_offset
            tft.fill_rect(x, y, white_highlight_w, white_highlight_h, cfg.palette[11])
        else:
            x = idx * white_w + white_w - black_w // 2
            tft.fill_rect(x, piano_y, black_w, black_h, cfg.palette[11])
            tft.rect(x, piano_y, black_w, black_h, cfg.palette[7])
        last_key = active_note_key
    else:
        last_key = None
    tft.show()

# ───────────── Rust OS Lifecycle ─────────────

def update(dt_ms, key_code, key_event):
    global octave, i2s, active_note_key, active_freq, phase, buffer
    
    if i2s is None:
        try:
            i2s = I2S(0, sck=Pin(41), ws=Pin(43), sd=Pin(42), mode=I2S.TX, bits=16, format=I2S.MONO, rate=SRATE, ibuf=CHUNK_SIZE * 4)
        except: pass
        draw_static_ui()

    if key_event == 1: # Pressed
        if key_code == 130: # UP
            octave = min(octave + 1, 2)
        elif key_code == 131: # DOWN
            octave = max(octave - 1, -2)
        elif key_code == 27: # ESC
            active_note_key = None
            active_freq = 0
        else:
            char = chr(key_code).upper() if 32 <= key_code <= 126 else ""
            if char in NOTES:
                active_note_key = char
                active_freq = get_freq(NOTES[char], octave)

    # Audio synthesis
    if active_freq > 0:
        # Generate sine wave chunk
        inc = 2 * math.pi * active_freq / SRATE
        for i in range(CHUNK_SIZE):
            val = int(math.sin(phase) * 32767)
            struct.pack_into('<h', buffer, i*2, val)
            phase += inc
            if phase > 2 * math.pi: phase -= 2 * math.pi
        
        try: i2s.write(buffer)
        except: pass
    
    update_display()

def last_error():
    return ""
