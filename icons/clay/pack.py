import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RENDERS = sys.argv[1]
SUBJECTS = json.load(open(os.path.join(HERE, "build", "subjects.json")))
THEME = os.path.join(HERE, "..", "drifter-icons")

for key, s in SUBJECTS.items():
    folder, name = s["path"].split("/")
    for pass_, root, depth in (("color", os.path.join(THEME, "256x256"), "8"), ("normal", os.path.join(THEME, "normal"), "8")):
        src = os.path.join(RENDERS, pass_, f"{key}.png")
        dst = os.path.join(root, folder, f"{name}.png")
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        if os.path.islink(dst):
            os.remove(dst)
        subprocess.run(["magick", src, "-filter", "Lanczos", "-resize", "256x256", "-depth", depth, "-strip", dst], check=True)
print(len(SUBJECTS))
