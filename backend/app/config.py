from functools import lru_cache
from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DATABASE_URL = f"sqlite:///{REPO_ROOT / 'data' / 'worklog.db'}"


class Settings(BaseSettings):
    app_name: str = "Worklog"
    timezone: str = "Asia/Shanghai"
    host: str = "127.0.0.1"
    port: int = 8000
    database_url: str = DEFAULT_DATABASE_URL
    cors_origins: list[str] = ["http://127.0.0.1:5173", "http://localhost:5173"]
    enable_scheduler: bool = True

    model_config = SettingsConfigDict(env_prefix="WORKLOG_", env_file=".env", extra="ignore")


@lru_cache
def get_settings() -> Settings:
    current = Settings()
    if current.database_url.startswith("sqlite:///"):
        db_path = current.database_url.replace("sqlite:///", "", 1)
        Path(db_path).expanduser().parent.mkdir(parents=True, exist_ok=True)
    return current


settings = get_settings()
