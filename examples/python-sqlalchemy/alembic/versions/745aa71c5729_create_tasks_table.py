"""create tasks table

Revision ID: 745aa71c5729
Revises:
Create Date: 2024-02-13 08:32:25.586605

"""

from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = "745aa71c5729"
down_revision: Union[str, None] = None
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    op.create_table(
        "tasks",
        sa.Column("id", sa.Integer, primary_key=True),
        sa.Column("task", sa.Text, nullable=False),
        sa.Column(
            "completed", sa.Boolean, nullable=False, server_default=sa.text("false")
        ),
    )


def downgrade() -> None:
    op.drop_table("tasks")
