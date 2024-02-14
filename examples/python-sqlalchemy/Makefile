.EXPORT_ALL_VARIABLES:
PGTEMP_SINGLE = postgresql://postgres@localhost:11432
PGTEMP_NORMAL = postgresql://postgres@localhost:22432

MYPY_ARGS = --non-interactive --install-types --check-untyped-defs

install:
	python -m venv venv
	./venv/bin/pip install -e .

# Note that you must have both a single and multi-mode pgtemp instance running for these tests
test:
	./venv/bin/mypy $(MYPY_ARGS) src/pgtemp_example_py/ tests/
	./venv/bin/black --check .
	./venv/bin/pytest

test-ci: install
	cargo build --features cli
	./../../target/debug/pgtemp $(PGTEMP_SINGLE) --single &
	./../../target/debug/pgtemp $(PGTEMP_NORMAL) &
	sleep 1 # wait until both are running
	./venv/bin/mypy $(MYPY_ARGS) src/pgtemp_example_py/ tests/
	./venv/bin/black --check .
	./venv/bin/pytest

clean:
	rm -rf venv/ build/ *egg-info
