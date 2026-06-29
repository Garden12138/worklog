"""add per-setting LLM timeout

Revision ID: 0004_llm_timeout_seconds
Revises: 0003_report_email_delivery
Create Date: 2026-06-29 00:00:00.000000
"""

from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


revision: str = "0004_llm_timeout_seconds"
down_revision: Union[str, None] = "0003_report_email_delivery"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.add_column(
        "llm_settings",
        sa.Column("timeout_seconds", sa.Integer(), nullable=False, server_default="60"),
    )
    op.execute("UPDATE llm_settings SET timeout_seconds = 180 WHERE provider = 'nvidia'")


def downgrade() -> None:
    op.drop_column("llm_settings", "timeout_seconds")
