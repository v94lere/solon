# Génère les petites icônes PNG (32×32, fond transparent) du menu de la barre des tâches.
# Usage : python make.py   (dépend de Pillow). Les fichiers produits sont versionnés.
from PIL import Image, ImageDraw
import os

S = 32
HERE = os.path.dirname(os.path.abspath(__file__))


def new():
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    return im, ImageDraw.Draw(im)


def save(im, name):
    im.save(os.path.join(HERE, name + ".png"))


def dot(color, name):
    im, d = new()
    d.ellipse((8, 8, 24, 24), fill=color)
    save(im, name)


GREY = (140, 146, 160, 255)
INK = (90, 96, 110, 255)
dot((31, 157, 75, 255), "dot-green")
dot(GREY, "dot-grey")
dot((222, 160, 40, 255), "dot-orange")
dot((198, 40, 40, 255), "dot-red")

# lecture (démarrer)
im, d = new()
d.polygon([(10, 6), (26, 16), (10, 26)], fill=INK)
save(im, "play")

# arrêt
im, d = new()
d.rounded_rectangle((8, 8, 24, 24), radius=3, fill=INK)
save(im, "stop")

# redémarrer : arc + flèche
im, d = new()
d.arc((6, 6, 26, 26), start=300, end=240, fill=INK, width=3)
d.polygon([(26, 4), (27, 13), (18, 11)], fill=INK)
save(im, "restart")

# ouvrir : fenêtre
im, d = new()
d.rounded_rectangle((5, 7, 27, 25), radius=3, outline=INK, width=3)
d.line((5, 12, 27, 12), fill=INK, width=3)
save(im, "open")

# quitter : croix
im, d = new()
d.line((9, 9, 23, 23), fill=INK, width=3)
d.line((23, 9, 9, 23), fill=INK, width=3)
save(im, "quit")

# conteneurs : cube (contour)
im, d = new()
d.polygon([(16, 4), (28, 10), (16, 16), (4, 10)], outline=INK, width=2)
d.line((4, 10, 4, 22, 16, 28, 28, 22, 28, 10), fill=INK, width=2)
d.line((16, 16, 16, 28), fill=INK, width=2)
save(im, "cube")

# moteur : éclair
im, d = new()
d.polygon([(18, 3), (8, 18), (15, 18), (13, 29), (24, 13), (17, 13)], fill=INK)
save(im, "bolt")
print("icônes du menu générées")
