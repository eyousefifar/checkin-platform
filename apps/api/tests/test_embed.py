"""Embedding pack/unpack + mean L2 — real shipped functions."""

from __future__ import annotations

import numpy as np
import pytest

from app.services.vision.embed import l2_normalize, mean_l2_embedding, pack_embedding, unpack_embedding


def test_pack_unpack_roundtrip():
    rng = np.random.default_rng(0)
    v = l2_normalize(rng.standard_normal(512).astype(np.float32))
    blob = pack_embedding(v)
    assert isinstance(blob, bytes)
    assert len(blob) == 512 * 4
    out = unpack_embedding(blob)
    assert out.shape == (512,)
    np.testing.assert_allclose(out, v, rtol=1e-5, atol=1e-6)
    assert abs(float(np.linalg.norm(out)) - 1.0) < 1e-5


def test_pack_wrong_dim_raises():
    with pytest.raises(ValueError):
        pack_embedding(np.ones(128, dtype=np.float32))


def test_mean_l2_embedding_unit_norm():
    rng = np.random.default_rng(1)
    vecs = [rng.standard_normal(512).astype(np.float32) for _ in range(5)]
    m = mean_l2_embedding(vecs)
    assert m.shape == (512,)
    assert abs(float(np.linalg.norm(m)) - 1.0) < 1e-5


def test_mean_empty_raises():
    with pytest.raises(ValueError):
        mean_l2_embedding([])
