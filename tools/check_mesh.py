"""Independent mesh check: trimesh and Manifold, not our own code.

Usage: uv run --with trimesh --with manifold3d --with numpy python -I tools/check_mesh.py FILE.stl...
Exit status 1 if any file is not one closed, consistently oriented solid.
"""

import sys

import manifold3d
import numpy as np
import trimesh


def check(path: str) -> bool:
    mesh = trimesh.load(path, force="mesh")
    bodies = mesh.split(only_watertight=False)
    solid = manifold3d.Manifold(
        manifold3d.Mesh(
            vert_properties=np.asarray(mesh.vertices, dtype=np.float32),
            tri_verts=np.asarray(mesh.faces, dtype=np.uint32),
        )
    )
    status = solid.status()
    ok = bool(
        mesh.is_watertight
        and mesh.is_winding_consistent
        and mesh.is_volume
        and len(bodies) == 1
        and status == manifold3d.Error.NoError
    )
    print(
        f"{path}: watertight={mesh.is_watertight} winding={mesh.is_winding_consistent} "
        f"volume_ok={mesh.is_volume} bodies={len(bodies)} euler={mesh.euler_number} "
        f"volume={mesh.volume:.3f} manifold={status.name} -> {'OK' if ok else 'FAIL'}"
    )
    return ok


if __name__ == "__main__":
    results = [check(path) for path in sys.argv[1:]]
    sys.exit(0 if results and all(results) else 1)
