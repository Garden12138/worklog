"""add work log date range

Revision ID: 0002_work_log_date_range
Revises: 0001_initial
Create Date: 2026-06-23 00:00:00.000000
"""

from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


revision: str = "0002_work_log_date_range"
down_revision: Union[str, None] = "0001_initial"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.add_column("work_logs", sa.Column("start_date", sa.Date(), nullable=True))
    op.add_column("work_logs", sa.Column("end_date", sa.Date(), nullable=True))
    op.execute("UPDATE work_logs SET start_date = work_date WHERE start_date IS NULL")
    op.execute("UPDATE work_logs SET end_date = work_date WHERE end_date IS NULL")
    op.create_index("ix_work_logs_start_date", "work_logs", ["start_date"])
    op.create_index("ix_work_logs_end_date", "work_logs", ["end_date"])


def downgrade() -> None:
    op.drop_index("ix_work_logs_end_date", table_name="work_logs")
    op.drop_index("ix_work_logs_start_date", table_name="work_logs")
    op.drop_column("work_logs", "end_date")
    op.drop_column("work_logs", "start_date")
