from collections.abc import Generator

from sqlalchemy import create_engine, text
from sqlalchemy.orm import DeclarativeBase, Session, sessionmaker

from app.config import settings


class Base(DeclarativeBase):
    pass


connect_args = {"check_same_thread": False} if settings.database_url.startswith("sqlite") else {}
engine = create_engine(settings.database_url, connect_args=connect_args)
SessionLocal = sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)


def get_db() -> Generator[Session, None, None]:
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()


def init_db() -> None:
    from app import models  # noqa: F401

    Base.metadata.create_all(bind=engine)
    ensure_sqlite_columns()


def ensure_sqlite_columns() -> None:
    if not settings.database_url.startswith("sqlite"):
        return
    with engine.begin() as connection:
        columns = {
            row[1]
            for row in connection.execute(text("PRAGMA table_info(work_logs)")).fetchall()
        }
        if "start_date" not in columns:
            connection.execute(text("ALTER TABLE work_logs ADD COLUMN start_date DATE"))
            connection.execute(text("UPDATE work_logs SET start_date = work_date WHERE start_date IS NULL"))
            connection.execute(
                text("CREATE INDEX IF NOT EXISTS ix_work_logs_start_date ON work_logs (start_date)")
            )
        if "end_date" not in columns:
            connection.execute(text("ALTER TABLE work_logs ADD COLUMN end_date DATE"))
            connection.execute(text("UPDATE work_logs SET end_date = work_date WHERE end_date IS NULL"))
            connection.execute(
                text("CREATE INDEX IF NOT EXISTS ix_work_logs_end_date ON work_logs (end_date)")
            )

        llm_columns = {
            row[1]
            for row in connection.execute(text("PRAGMA table_info(llm_settings)")).fetchall()
        }
        if llm_columns and "timeout_seconds" not in llm_columns:
            connection.execute(
                text("ALTER TABLE llm_settings ADD COLUMN timeout_seconds INTEGER NOT NULL DEFAULT 60")
            )
            connection.execute(
                text("UPDATE llm_settings SET timeout_seconds = 180 WHERE provider = 'nvidia'")
            )
