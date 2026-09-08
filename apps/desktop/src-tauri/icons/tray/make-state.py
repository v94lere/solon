# Icônes de la barre des tâches qui reflètent l'état du moteur : le cube Monodon (icons/32x32.png)
# avec un point coloré en bas à droite. Usage : python make-state.py (dépend de Pillow).
import os
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
CUBE = os.path.join(HERE, "..", "32x32.png")
STATES = {
    "tray-ready": (31, 157, 75, 255),
    "tray-busy": (222, 160, 40, 255),
    "tray-failed": (198, 40, 40, 255),
    "tray-stopped": (140, 146, 160, 255),
}
base = Image.open(CUBE).convert("RGBA")
for name, color in STATES.items():
    im = base.copy()
    d = ImageDraw.Draw(im)
    # Liseré transparent pour détacher le point du cube, puis le point.
    d.ellipse((18, 18, 32, 32), fill=(0, 0, 0, 0))
    d.ellipse((20, 20, 30, 30), fill=color)
    im.save(os.path.join(HERE, name + ".png"))
    print(name)
