#!/usr/bin/env python3
"""Optional seed: create demo employees without faces (or with synthetic vectors)."""

from __future__ import annotations

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "apps" / "api"))
os.chdir(ROOT / "apps" / "api")

from app.config import get_settings
from app.db.models import Employee
from app.db.session import SessionLocal, init_db
from sqlalchemy import select


def main() -> None:
    get_settings.cache_clear()
    init_db()
    assert SessionLocal is not None
    db = SessionLocal()
    try:
        seeds = [
            ("E1001", "Alice Demo", "Engineering"),
            ("E1002", "Bob Demo", "Operations"),
            ("E1003", "Cara Demo", "Design"),
        ]
        for code, name, dept in seeds:
            exists = db.scalars(select(Employee).where(Employee.employee_code == code)).first()
            if exists:
                print(f"skip {code}")
                continue
            db.add(Employee(employee_code=code, full_name=name, department=dept))
            print(f"created {code} {name}")
        db.commit()
        print("Done. Upload enrollment photos via UI or API.")
    finally:
        db.close()


if __name__ == "__main__":
    main()
