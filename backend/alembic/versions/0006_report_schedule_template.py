"""add template selection to report schedules

Revision ID: 0006_report_schedule_template
Revises: 0005_report_schedules
Create Date: 2026-06-29 00:00:00.000000
"""

from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


revision: str = "0006_report_schedule_template"
down_revision: Union[str, None] = "0005_report_schedules"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    with op.batch_alter_table("report_schedules") as batch_op:
        batch_op.add_column(sa.Column("template_id", sa.Integer(), nullable=True))
        batch_op.create_foreign_key(
            "fk_report_schedules_template_id_templates",
            "templates",
            ["template_id"],
            ["id"],
            ondelete="SET NULL",
        )


def downgrade() -> None:
    with op.batch_alter_table("report_schedules") as batch_op:
        batch_op.drop_constraint(
            "fk_report_schedules_template_id_templates",
            type_="foreignkey",
        )
        batch_op.drop_column("template_id")
