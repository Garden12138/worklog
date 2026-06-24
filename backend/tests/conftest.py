import os
import tempfile

os.environ.setdefault("WORKLOG_DATABASE_URL", f"sqlite:///{tempfile.mkdtemp()}/worklog-test.db")
os.environ.setdefault("WORKLOG_ENABLE_SCHEDULER", "false")

import pytest
from fastapi.testclient import TestClient

from app.database import Base, SessionLocal, engine
from app.main import app
from app.services.reports import seed_default_templates


@pytest.fixture()
def db_session():
    Base.metadata.drop_all(bind=engine)
    Base.metadata.create_all(bind=engine)
    with SessionLocal() as db:
        seed_default_templates(db)
        yield db


@pytest.fixture()
def client(db_session):
    with TestClient(app) as test_client:
        yield test_client
