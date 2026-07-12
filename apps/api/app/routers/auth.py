from __future__ import annotations

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from app.auth import create_token
from app.config import get_settings

router = APIRouter(prefix="/auth", tags=["auth"])


class LoginBody(BaseModel):
    password: str


@router.post("/login")
def login(body: LoginBody) -> dict:
    settings = get_settings()
    if body.password != settings.admin_password:
        raise HTTPException(status_code=401, detail="Invalid password")
    token = create_token()
    return {"access_token": token, "token_type": "bearer"}
