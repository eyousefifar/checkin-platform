"""In-memory gallery matrix for cosine match."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.db.models import AppMeta, Employee, EmployeeEmbedding
from app.services.vision.embed import unpack_embedding
from app.services.vision.match import MatchResult, match_top1


@dataclass
class GalleryService:
    employee_ids: list[int] = field(default_factory=list)
    names: list[str] = field(default_factory=list)
    matrix: np.ndarray = field(default_factory=lambda: np.zeros((0, 512), dtype=np.float32))
    version: int = 0
    threshold: float = 0.45
    margin: float = 0.08
    dim: int = 512

    def size(self) -> int:
        return len(self.employee_ids)

    def load(self, db: Session) -> None:
        rows = db.execute(
            select(Employee, EmployeeEmbedding)
            .join(EmployeeEmbedding, EmployeeEmbedding.employee_id == Employee.id)
            .where(Employee.is_active.is_(True))
        ).all()
        ids: list[int] = []
        names: list[str] = []
        vecs: list[np.ndarray] = []
        for emp, emb in rows:
            try:
                v = unpack_embedding(emb.vector, dim=emb.dim or self.dim)
            except ValueError:
                continue
            ids.append(emp.id)
            names.append(emp.full_name)
            vecs.append(v)
        self.employee_ids = ids
        self.names = names
        if vecs:
            self.matrix = np.stack(vecs).astype(np.float32)
        else:
            self.matrix = np.zeros((0, self.dim), dtype=np.float32)
        meta = db.get(AppMeta, "gallery_version")
        self.version = int(meta.value) if meta and meta.value.isdigit() else 0

    def match(self, query: np.ndarray) -> MatchResult:
        return match_top1(
            query,
            self.matrix,
            self.employee_ids,
            self.names,
            threshold=self.threshold,
            margin=self.margin,
        )

    def bump_version(self, db: Session) -> int:
        meta = db.get(AppMeta, "gallery_version")
        if meta is None:
            meta = AppMeta(key="gallery_version", value="0")
            db.add(meta)
        self.version = int(meta.value or "0") + 1
        meta.value = str(self.version)
        db.commit()
        return self.version


# process-global gallery
_gallery: GalleryService | None = None


def get_gallery() -> GalleryService:
    global _gallery
    if _gallery is None:
        from app.config import get_settings

        s = get_settings()
        _gallery = GalleryService(threshold=s.match_threshold, margin=s.match_margin, dim=s.embedding_dim)
    return _gallery


def reset_gallery() -> None:
    global _gallery
    _gallery = None
