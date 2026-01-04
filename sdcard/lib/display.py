import cardputer

class Display:
    def __init__(self):
        self.width = cardputer.screen_width()
        self.height = cardputer.screen_height()

    def fill(self, color):
        cardputer.clear(color)

    def fill_rect(self, x, y, w, h, color):
        cardputer.fill_rect(x, y, w, h, color)

    def rect(self, x, y, w, h, color):
        # Fallback to lines or thin rects if cardputer.rect is missing
        # Here we just draw a hollow box using 1px rects
        cardputer.fill_rect(x, y, w, 1, color)
        cardputer.fill_rect(x, y + h - 1, w, 1, color)
        cardputer.fill_rect(x, y, 1, h, color)
        cardputer.fill_rect(x + w - 1, y, 1, h, color)

    def text(self, msg, x, y, color, font=None):
        # Built-in cardputer module doesn't have text support in the bridge yet.
        # This will be a stub until we expose a font renderer.
        pass

    def show(self):
        cardputer.present()
