from alembic.config import Config
from alembic import command

import pytest

from sqlalchemy import create_engine, text
from sqlalchemy.pool import NullPool
from sqlalchemy.orm import Session
from sqlalchemy.engine import Engine

import os

import random

from typing import Generator

from pgtemp_example_py import insert_task, list_tasks, complete_task


def get_db_and_run_migrations() -> tuple[Engine, str]:
    """
    Given an existing pgtemp server running in single mode with its url in the
    `PGTEMP_SINGLE` environment variable, and that the make a new database for
    testing and return a sqlalchemy `Engine` that can connect to it.
    """
    db_url = os.environ["PGTEMP_SINGLE"]

    # connect to the db
    engine = create_engine(db_url)

    # make a new database
    db_id = random.randint(0, 1000000)
    db_name = f"test{db_id}"
    with engine.connect().execution_options(isolation_level="AUTOCOMMIT") as conn:
        conn.execute(text(f"CREATE DATABASE {db_name};"))

    print(f"database name: `{db_name}`")

    db_url += "/" + db_name

    # make a new config with the new db url and return the connection
    cfg = Config("alembic.ini")
    cfg.set_main_option("sqlalchemy.url", db_url)

    command.upgrade(cfg, "head")
    # NullPool isn't necessary for single mode because either way it will
    # connect to the same database, but we enable it here as a demonstration
    # that even if pooling is disabled, each connection will still connect to
    # the same database.
    engine = create_engine(db_url, poolclass=NullPool)

    return engine, db_name


def drop_db(db_name: str):
    """Drop a test database created via the get_db function"""
    db_url = os.environ["PGTEMP_SINGLE"]
    engine = create_engine(db_url)
    with engine.connect().execution_options(isolation_level="AUTOCOMMIT") as conn:
        conn.execute(text(f"DROP DATABASE {db_name}"))


@pytest.fixture
def test_db() -> Generator[Engine, None, None]:
    """
    A sample fixture that connects to a pgtemp instance in single mode,
    sets up a test database, and drops it when the test finishes.

    This is probably faster than spinning up a new db per connection like
    pgtemp without single mode, and allows you to test multiple connections and
    connection pools, but requires a slightly more complicated fixture.
    """
    db, db_name = get_db_and_run_migrations()
    yield db
    db.dispose()
    drop_db(db_name)


def test_basic_ops(test_db):
    """
    Test insert/query/complete task functions with separate connections each time.
    This is closer to how code would run if it were behind e.g. a web service,
    but also note that we've set up sqlalchemy Engine to not use pooling.
    """
    with Session(test_db) as session:
        # insert a task and check it's in the db
        insert_task(session, "hello")
        session.commit()

    with Session(test_db) as session:
        tasks = list_tasks(session)

        assert len(tasks) == 1

        t = tasks[0]
        tid = tasks[0].id

        assert t.task == "hello"
        assert t.completed is False
        session.commit()

    with Session(test_db) as session:
        # complete task and check it's marked as completed
        complete_task(session, tid)
        session.commit()

    with Session(test_db) as session:
        tasks = list_tasks(session)
        assert len(tasks) == 1
        t = tasks[0]
        assert t.task == "hello"
        assert t.completed is True
        session.commit()
