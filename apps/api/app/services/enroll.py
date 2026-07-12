"""Single enrollment pipeline: images → embeddings → mean → gallery bump."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from uuid import uuid4

import cv2
import numpy as np
from sqlalchemy.orm import Session

from app.config import Settings, get_settings
from app.db.models import Employee, EmployeeEmbedding, EmployeeImage, utcnow
from app.services.gallery.service import get_gallery
from app.services.vision.embed import l2_normalize, mean_l2_embedding, pack_embedding
from app.services.vision.engine import FaceEngine, get_face_engine
from app.services.vision.quality import quality_gate


@dataclass
class RejectedImage:
    filename: str
    reason: str


@dataclass
class EnrollResult:
    received: int
    usable: int
    rejected: list[RejectedImage]
    embedding_ready: bool
    num_images_used: int


def extract_embedding_from_bgr(
    image_bgr: np.ndarray,
    engine: FaceEngine,
    settings: Settings | None = None,
) -> tuple[np.ndarray | None, str | None]:
    """Return (embedding, reject_reason)."""
    settings = settings or get_settings()
    faces = engine.get(image_bgr)
    if not faces:
        return None, "no_face"
    # pick largest face
    faces = sorted(
        faces,
        key=lambda f: (f.bbox[2] - f.bbox[0]) * (f.bbox[3] - f.bbox[1]),
        reverse=True,
    )
    face = faces[0]
    h, w = image_bgr.shape[:2]
    q = quality_gate(
        face.det_score,
        face.bbox,
        min_det_score=settings.min_det_score,
        min_face_px=settings.min_face_px,
        frame_w=w,
        frame_h=h,
        bbox_normalized=False,
    )
    if not q.ok:
        return None, q.reason or "low_quality"
    return l2_normalize(face.embedding), None


def process_upload_bytes(
    data: bytes,
    filename: str,
    engine: FaceEngine,
    settings: Settings | None = None,
) -> tuple[np.ndarray | None, str | None, np.ndarray | None]:
    """Decode image, extract embedding. Returns (emb, reason, bgr)."""
    arr = np.frombuffer(data, dtype=np.uint8)
    bgr = cv2.imdecode(arr, cv2.IMREAD_COLOR)
    if bgr is None:
        return None, "decode_error", None
    emb, reason = extract_embedding_from_bgr(bgr, engine, settings)
    return emb, reason, bgr


def recompute_embedding(
    db: Session,
    employee: Employee,
    engine: FaceEngine | None = None,
    settings: Settings | None = None,
) -> EnrollResult:
    settings = settings or get_settings()
    engine = engine or get_face_engine()
    vectors: list[np.ndarray] = []
    rejected: list[RejectedImage] = []
    usable = 0

    for img in employee.images:
        path = (
            Path(img.file_path)
            if Path(img.file_path).is_absolute()
            else settings.resolved_data_dir / img.file_path
        )
        # also try relative to repo data
        if not path.exists():
            path = settings.enroll_dir / str(employee.id) / Path(img.file_path).name
        if not path.exists():
            img.usable = False
            img.reject_reason = "missing_file"
            rejected.append(RejectedImage(filename=path.name, reason="missing_file"))
            continue
        bgr = cv2.imread(str(path))
        if bgr is None:
            img.usable = False
            img.reject_reason = "decode_error"
            rejected.append(RejectedImage(filename=path.name, reason="decode_error"))
            continue
        emb, reason = extract_embedding_from_bgr(bgr, engine, settings)
        if emb is None:
            img.usable = False
            img.reject_reason = reason
            rejected.append(RejectedImage(filename=path.name, reason=reason or "no_face"))
        else:
            img.usable = True
            img.reject_reason = None
            vectors.append(emb)
            usable += 1

    embedding_ready = False
    num_used = 0
    if len(vectors) >= settings.min_enroll_images:
        mean = mean_l2_embedding(vectors, dim=settings.embedding_dim)
        blob = pack_embedding(mean, dim=settings.embedding_dim)
        row = db.get(EmployeeEmbedding, employee.id)
        if row is None:
            row = EmployeeEmbedding(employee_id=employee.id, vector=blob, dim=settings.embedding_dim)
            db.add(row)
        else:
            row.vector = blob
            row.dim = settings.embedding_dim
        row.num_images_used = len(vectors)
        row.model_name = getattr(engine, "model_name", settings.model_name)
        row.updated_at = utcnow()
        embedding_ready = True
        num_used = len(vectors)
    elif employee.embedding is not None:
        # clear if not enough
        db.delete(employee.embedding)

    gallery = get_gallery()
    gallery.bump_version(db)
    db.commit()
    gallery.load(db)
    try:
        from app.ws.hub import hub

        hub.gallery_version = gallery.version
    except Exception:  # noqa: BLE001
        pass

    return EnrollResult(
        received=len(employee.images),
        usable=usable,
        rejected=rejected,
        embedding_ready=embedding_ready,
        num_images_used=num_used,
    )


def save_images_and_enroll(
    db: Session,
    employee: Employee,
    files: list[tuple[str, bytes]],
    engine: FaceEngine | None = None,
    settings: Settings | None = None,
) -> EnrollResult:
    settings = settings or get_settings()
    engine = engine or get_face_engine()
    dest = settings.enroll_dir / str(employee.id)
    dest.mkdir(parents=True, exist_ok=True)

    rejected: list[RejectedImage] = []
    usable = 0
    received = len(files)

    for filename, data in files:
        emb, reason, bgr = process_upload_bytes(data, filename, engine, settings)
        uid = uuid4().hex[:12]
        ext = Path(filename).suffix.lower() or ".jpg"
        if ext not in (".jpg", ".jpeg", ".png", ".webp", ".bmp"):
            ext = ".jpg"
        rel = f"enroll/{employee.id}/{uid}{ext}"
        abs_path = settings.resolved_data_dir / rel
        abs_path.parent.mkdir(parents=True, exist_ok=True)

        if bgr is not None:
            cv2.imwrite(str(abs_path), bgr)
        else:
            abs_path.write_bytes(data)

        ok = emb is not None
        img = EmployeeImage(
            employee_id=employee.id,
            file_path=rel,
            usable=ok,
            reject_reason=None if ok else (reason or "no_face"),
        )
        db.add(img)
        if ok:
            usable += 1
        else:
            rejected.append(RejectedImage(filename=filename, reason=reason or "no_face"))

    db.commit()
    db.refresh(employee)
    # reload images
    _ = employee.images
    return recompute_embedding(db, employee, engine=engine, settings=settings)
