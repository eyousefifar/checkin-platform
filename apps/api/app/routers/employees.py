from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, File, HTTPException, UploadFile
from pydantic import BaseModel, Field
from sqlalchemy import select
from sqlalchemy.orm import Session

from app.auth import require_auth
from app.db.models import Employee, utcnow
from app.db.session import get_db
from app.services.enroll import save_images_and_enroll, recompute_embedding
from app.services.vision.engine import get_face_engine

router = APIRouter(prefix="/employees", tags=["employees"])


class EmployeeCreate(BaseModel):
    employee_code: str = Field(min_length=1, max_length=64)
    full_name: str = Field(min_length=1, max_length=256)
    department: str | None = None


class EmployeeUpdate(BaseModel):
    full_name: str | None = None
    department: str | None = None
    is_active: bool | None = None


def _emp_dict(e: Employee) -> dict:
    usable = sum(1 for i in e.images if i.usable)
    return {
        "id": e.id,
        "employee_code": e.employee_code,
        "full_name": e.full_name,
        "department": e.department,
        "is_active": e.is_active,
        "image_count": len(e.images),
        "usable_images": usable,
        "embedding_ready": e.embedding is not None,
        "num_images_used": e.embedding.num_images_used if e.embedding else 0,
        "images": [
            {
                "id": i.id,
                "file_path": i.file_path,
                "usable": i.usable,
                "reject_reason": i.reject_reason,
            }
            for i in e.images
        ],
    }


@router.get("")
def list_employees(
    q: str | None = None,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> list[dict]:
    stmt = select(Employee).order_by(Employee.full_name)
    rows = db.scalars(stmt).all()
    if q:
        ql = q.lower()
        rows = [e for e in rows if ql in e.full_name.lower() or ql in e.employee_code.lower()]
    return [_emp_dict(e) for e in rows]


@router.post("", status_code=201)
def create_employee(
    body: EmployeeCreate,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    exists = db.scalars(select(Employee).where(Employee.employee_code == body.employee_code)).first()
    if exists:
        raise HTTPException(status_code=409, detail="Employee code already exists")
    emp = Employee(
        employee_code=body.employee_code,
        full_name=body.full_name,
        department=body.department,
    )
    db.add(emp)
    db.commit()
    db.refresh(emp)
    return _emp_dict(emp)


@router.get("/{employee_id}")
def get_employee(
    employee_id: int,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    emp = db.get(Employee, employee_id)
    if not emp:
        raise HTTPException(status_code=404, detail="Not found")
    return _emp_dict(emp)


@router.patch("/{employee_id}")
def update_employee(
    employee_id: int,
    body: EmployeeUpdate,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    emp = db.get(Employee, employee_id)
    if not emp:
        raise HTTPException(status_code=404, detail="Not found")
    if body.full_name is not None:
        emp.full_name = body.full_name
    if body.department is not None:
        emp.department = body.department
    if body.is_active is not None:
        emp.is_active = body.is_active
    emp.updated_at = utcnow()
    db.commit()
    db.refresh(emp)
    return _emp_dict(emp)


@router.delete("/{employee_id}")
def delete_employee(
    employee_id: int,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    emp = db.get(Employee, employee_id)
    if not emp:
        raise HTTPException(status_code=404, detail="Not found")
    emp.is_active = False
    emp.updated_at = utcnow()
    db.commit()
    return {"ok": True}


@router.post("/{employee_id}/images")
async def upload_images(
    employee_id: int,
    files: Annotated[list[UploadFile], File(...)],
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    emp = db.get(Employee, employee_id)
    if not emp:
        raise HTTPException(status_code=404, detail="Not found")
    payloads: list[tuple[str, bytes]] = []
    for f in files:
        data = await f.read()
        payloads.append((f.filename or "image.jpg", data))
    result = save_images_and_enroll(db, emp, payloads, engine=get_face_engine())
    return {
        "received": result.received,
        "usable": result.usable,
        "rejected": [{"filename": r.filename, "reason": r.reason} for r in result.rejected],
        "embedding_ready": result.embedding_ready,
        "num_images_used": result.num_images_used,
    }


@router.post("/{employee_id}/recompute-embedding")
def recompute(
    employee_id: int,
    db: Session = Depends(get_db),
    _auth: dict = Depends(require_auth),
) -> dict:
    emp = db.get(Employee, employee_id)
    if not emp:
        raise HTTPException(status_code=404, detail="Not found")
    result = recompute_embedding(db, emp, engine=get_face_engine())
    return {
        "received": result.received,
        "usable": result.usable,
        "rejected": [{"filename": r.filename, "reason": r.reason} for r in result.rejected],
        "embedding_ready": result.embedding_ready,
        "num_images_used": result.num_images_used,
    }
