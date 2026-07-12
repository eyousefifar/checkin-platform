"""SQLAlchemy models — SQLite."""

from __future__ import annotations

from datetime import date, datetime, timezone

from sqlalchemy import (
    Boolean,
    Date,
    DateTime,
    Float,
    ForeignKey,
    Integer,
    LargeBinary,
    String,
    Text,
    Index,
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


def utcnow() -> datetime:
    return datetime.now(timezone.utc).replace(tzinfo=None)


class Base(DeclarativeBase):
    pass


class Employee(Base):
    __tablename__ = "employees"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    employee_code: Mapped[str] = mapped_column(String(64), unique=True, nullable=False)
    full_name: Mapped[str] = mapped_column(String(256), nullable=False)
    department: Mapped[str | None] = mapped_column(String(128), nullable=True)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=utcnow)
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=utcnow, onupdate=utcnow)

    images: Mapped[list[EmployeeImage]] = relationship(back_populates="employee", cascade="all, delete-orphan")
    embedding: Mapped[EmployeeEmbedding | None] = relationship(
        back_populates="employee", uselist=False, cascade="all, delete-orphan"
    )
    events: Mapped[list[AttendanceEvent]] = relationship(back_populates="employee")


class EmployeeImage(Base):
    __tablename__ = "employee_images"

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    employee_id: Mapped[int] = mapped_column(ForeignKey("employees.id", ondelete="CASCADE"), nullable=False)
    file_path: Mapped[str] = mapped_column(Text, nullable=False)
    usable: Mapped[bool] = mapped_column(Boolean, default=False)
    reject_reason: Mapped[str | None] = mapped_column(String(64), nullable=True)
    created_at: Mapped[datetime] = mapped_column(DateTime, default=utcnow)

    employee: Mapped[Employee] = relationship(back_populates="images")


class EmployeeEmbedding(Base):
    __tablename__ = "employee_embeddings"

    employee_id: Mapped[int] = mapped_column(ForeignKey("employees.id", ondelete="CASCADE"), primary_key=True)
    dim: Mapped[int] = mapped_column(Integer, default=512)
    vector: Mapped[bytes] = mapped_column(LargeBinary, nullable=False)
    num_images_used: Mapped[int] = mapped_column(Integer, default=0)
    model_name: Mapped[str] = mapped_column(String(64), default="buffalo_l")
    updated_at: Mapped[datetime] = mapped_column(DateTime, default=utcnow, onupdate=utcnow)

    employee: Mapped[Employee] = relationship(back_populates="embedding")


class Camera(Base):
    __tablename__ = "cameras"

    id: Mapped[str] = mapped_column(String(64), primary_key=True)
    name: Mapped[str] = mapped_column(String(128), nullable=False)
    rtsp_url: Mapped[str] = mapped_column(Text, default="")
    webrtc_path: Mapped[str] = mapped_column(String(128), default="")
    direction: Mapped[str] = mapped_column(String(32), default="in")
    enabled: Mapped[bool] = mapped_column(Boolean, default=True)
    sort_order: Mapped[int] = mapped_column(Integer, default=0)


class AttendanceEvent(Base):
    __tablename__ = "attendance_events"
    __table_args__ = (
        Index("ix_events_local_date_employee", "local_date", "employee_id"),
        Index("ix_events_ts", "ts"),
        Index("ix_events_emp_cam_ts", "employee_id", "camera_id", "ts"),
    )

    id: Mapped[int] = mapped_column(Integer, primary_key=True, autoincrement=True)
    employee_id: Mapped[int | None] = mapped_column(ForeignKey("employees.id"), nullable=True)
    camera_id: Mapped[str] = mapped_column(ForeignKey("cameras.id"), nullable=False)
    kind: Mapped[str] = mapped_column(String(32), nullable=False)
    score: Mapped[float | None] = mapped_column(Float, nullable=True)
    margin: Mapped[float | None] = mapped_column(Float, nullable=True)
    track_id: Mapped[int | None] = mapped_column(Integer, nullable=True)
    needs_review: Mapped[bool] = mapped_column(Boolean, default=False)
    meta_json: Mapped[str | None] = mapped_column(Text, nullable=True)
    ts: Mapped[datetime] = mapped_column(DateTime, nullable=False, default=utcnow)
    local_date: Mapped[date] = mapped_column(Date, nullable=False)

    employee: Mapped[Employee | None] = relationship(back_populates="events")
    camera: Mapped[Camera] = relationship()


class AppMeta(Base):
    __tablename__ = "app_meta"

    key: Mapped[str] = mapped_column(String(64), primary_key=True)
    value: Mapped[str] = mapped_column(Text, default="")
