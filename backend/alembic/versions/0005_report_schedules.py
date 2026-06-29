"""add configurable report schedules

Revision ID: 0005_report_schedules
Revises: 0004_llm_timeout_seconds
Create Date: 2026-06-29 00:00:00.000000
"""

from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


revision: str = "0005_report_schedules"
down_revision: Union[str, None] = "0004_llm_timeout_seconds"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "report_schedules",
        sa.Column("id", sa.Integer(), nullable=False),
        sa.Column("report_type", sa.String(length=48), nullable=False),
        sa.Column("enabled", sa.Boolean(), nullable=False),
        sa.Column("weekday", sa.String(length=3), nullable=True),
        sa.Column("day_of_month", sa.Integer(), nullable=True),
        sa.Column("run_time", sa.Time(), nullable=False),
        sa.Column("auto_send", sa.Boolean(), nullable=False),
        sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        sa.Column("updated_at", sa.DateTime(timezone=True), nullable=False),
        sa.PrimaryKeyConstraint("id"),
        sa.UniqueConstraint("report_type", name="uq_report_schedules_report_type"),
    )
    op.create_index("ix_report_schedules_report_type", "report_schedules", ["report_type"])
    op.create_table(
        "report_schedule_recipients",
        sa.Column("schedule_id", sa.Integer(), nullable=False),
        sa.Column("recipient_id", sa.Integer(), nullable=False),
        sa.ForeignKeyConstraint(["recipient_id"], ["recipients.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["schedule_id"], ["report_schedules.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("schedule_id", "recipient_id"),
    )


def downgrade() -> None:
    op.drop_table("report_schedule_recipients")
    op.drop_index("ix_report_schedules_report_type", table_name="report_schedules")
    op.drop_table("report_schedules")
