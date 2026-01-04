class Config:
    def __init__(self):
        # 16-color palette (RGB565 approximations)
        self.palette = [
            0x0000, # 0: Black
            0x001F, # 1: Blue
            0x07E0, # 2: Green
            0x07FF, # 3: Cyan
            0xF800, # 4: Red
            0xF81F, # 5: Magenta
            0xFFE0, # 6: Yellow
            0xFFFF, # 7: White
            0x7BEF, # 8: Grey
            0x000F, # 9: Dark Blue
            0x03E0, # 10: Dark Green
            0x7FFF, # 11: Light Blue
            0x7800, # 12: Dark Red
            0x780F, # 13: Dark Magenta
            0x7BE0, # 14: Dark Yellow
            0xBDF7, # 15: Silver
        ]
