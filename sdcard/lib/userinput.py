import cardputer

class UserInput:
    def __init__(self):
        self.last_keys = []

    def get_new_keys(self):
        # The Rust OS passes key events to update(), but we can also poll
        # if the bridge supports it. The cardputer_module.c has cardputer_poll_key.
        keys = []
        while True:
            k = cardputer.poll_key()
            if k is None:
                break
            # k is (code, event)
            # We need to map code to string names if the script expects strings
            char = chr(k[0]) if 32 <= k[0] <= 126 else str(k[0])
            if k[1] == 1: # Pressed
                keys.append(char)
        return keys

    def ext_dir_keys(self, keys):
        # Stub for extended directional keys
        pass
