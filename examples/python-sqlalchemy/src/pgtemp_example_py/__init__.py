from sqlalchemy import select, insert, update
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column


class Base(DeclarativeBase):
    pass


class Task(Base):
    __tablename__ = "tasks"

    id: Mapped[int] = mapped_column(primary_key=True)
    task: Mapped[str]
    completed: Mapped[bool]


def list_tasks(session: Session) -> list[Task]:
    return list(session.scalars(select(Task)).all())


def insert_task(session: Session, task: str):
    session.execute(insert(Task).values(task=task))


def complete_task(session: Session, id: int):
    session.execute(update(Task).where(Task.id == id).values(completed=True))
