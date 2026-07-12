"""Embedding pack/unpack and mean L2 vector — pure NumPy, no model."""

from __future__ import annotations

import numpy as np


def l2_normalize(vec: np.ndarray, eps: float = 1e-12) -> np.ndarray:
    v = np.asarray(vec, dtype=np.float32).reshape(-1)
    n = float(np.linalg.norm(v))
    if n < eps:
        return v
    return (v / n).astype(np.float32)


def pack_embedding(vec: np.ndarray, dim: int = 512) -> bytes:
    v = np.asarray(vec, dtype=np.float32).reshape(-1)
    if v.shape[0] != dim:
        raise ValueError(f"expected dim {dim}, got {v.shape[0]}")
    return v.astype(np.float32).tobytes(order="C")


def unpack_embedding(blob: bytes, dim: int = 512) -> np.ndarray:
    arr = np.frombuffer(blob, dtype=np.float32).copy()
    if arr.shape[0] != dim:
        raise ValueError(f"expected dim {dim}, got {arr.shape[0]}")
    return l2_normalize(arr)


def mean_l2_embedding(vectors: list[np.ndarray], dim: int = 512) -> np.ndarray:
    if not vectors:
        raise ValueError("no vectors to average")
    stacked = np.stack([l2_normalize(v) for v in vectors])
    if stacked.shape[1] != dim:
        raise ValueError(f"expected dim {dim}, got {stacked.shape[1]}")
    mean = stacked.mean(axis=0)
    return l2_normalize(mean)
