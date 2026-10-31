from PIL import Image, ImageDraw
import math

S = 256
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

def rr(box, r, fill):
    d.rounded_rectangle(box, radius=r, fill=fill)

# App tile
rr((8, 8, S-8, S-8), 56, (14, 16, 21, 255))
rr((8, 8, S-8, S-8), 56, None)

# Inner photo frame
rr((44, 52, S-44, S-60), 26, (24, 29, 38, 255))

# Sun
d.ellipse((150, 78, 190, 118), fill=(255, 196, 92, 255))

# Mountains
d.polygon([(52, 186), (104, 118), (146, 168), (170, 142), (204, 186)], fill=(74, 144, 226, 255))
d.polygon([(52, 186), (96, 140), (128, 186)], fill=(110, 178, 255, 255))

# Horizon line
d.rectangle((52, 186, 204, 190), fill=(180, 205, 235, 255))

img.save(r"C:\Users\Jonathan\Documents\deepseek-harness\default-workspace\bladr\assets\icon.png")
img.save(
    r"C:\Users\Jonathan\Documents\deepseek-harness\default-workspace\bladr\assets\icon.ico",
    sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (24, 24), (16, 16)],
)
print("ok")
