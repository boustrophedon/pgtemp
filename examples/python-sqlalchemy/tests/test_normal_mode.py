from alembic.config import Config
from alembic import command

import pytest

from sqlalchemy import create_engine, text
from sqlalchemy.orm import Session
from sqlalchemy.pool import NullPool

import os

from typing import Generator

from pgtemp_example_py import insert_task, list_tasks, complete_task


@pytest.fixture
def session() -> Generator[Session, None, None]:
    """
    A test fixture that connects to an existing pgtemp server running in normal
    (multi-cluster) mode from the URL in the `PGTEMP_NORMAL` environment
    variable, runs migrations, and returns a Session.

    Maybe simpler to set up than single mode (although you still have to modify
    alembic/env.py) but can be more expensive to run. Best for testing db
    operations directly on a Session.
    """
    db_url = os.environ["PGTEMP_NORMAL"]

    # connect to the db
    engine = create_engine(db_url)

    # make a connection, provide it to alembic, and run migrations
    with engine.connect() as conn:
        cfg = Config("alembic.ini")
        cfg.set_main_option("sqlalchemy.url", db_url)
        cfg.attributes["connection"] = conn
        command.upgrade(cfg, "head")
        yield Session(conn)


def test_basic_ops(session):
    """
    Test insert/query/complete task functions with a single connection.
    """
    # insert a task and check it's in the db
    insert_task(session, "hello")
    session.flush()

    tasks = list_tasks(session)
    session.flush()

    assert len(tasks) == 1
    t = tasks[0]
    assert t.task == "hello"
    assert t.completed is False

    # complete task and check it's marked as completed
    complete_task(session, t.id)
    session.flush()

    tasks = list_tasks(session)
    assert len(tasks) == 1
    t = tasks[0]
    assert t.task == "hello"
    assert t.completed is True
