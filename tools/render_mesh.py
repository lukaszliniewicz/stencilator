"""Renders an STL to a PNG with flat shading, for looking at meshes.

Usage: uv run --with trimesh --with matplotlib --with numpy python -I tools/render_mesh.py IN.stl OUT.png [elevation azimuth]
"""

import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import trimesh
from mpl_toolkits.mplot3d.art3d import Poly3DCollection

mesh = trimesh.load(sys.argv[1], force="mesh")
elevation, azimuth = (float(sys.argv[3]), float(sys.argv[4])) if len(sys.argv) > 4 else (35.0, -60.0)
light = np.array([0.3, -0.5, 0.8])
light /= np.linalg.norm(light)
shade = 0.35 + 0.65 * np.clip(mesh.face_normals @ light, 0, 1)
colours = np.column_stack([0.55 * shade, 0.62 * shade, 0.72 * shade, np.ones_like(shade)])
fig = plt.figure(figsize=(9, 7), dpi=150)
ax = fig.add_subplot(projection="3d")
ax.add_collection3d(Poly3DCollection(mesh.triangles, facecolors=colours, edgecolors="none"))
lo, hi = mesh.bounds
ax.set_xlim(lo[0], hi[0])
ax.set_ylim(lo[1], hi[1])
ax.set_zlim(lo[2], hi[2])
ax.set_box_aspect(hi - lo)
ax.view_init(elev=elevation, azim=azimuth)
ax.set_axis_off()
fig.savefig(sys.argv[2], bbox_inches="tight")
