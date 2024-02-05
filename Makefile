build:
	cargo build --all-features

# Run all tests and examples
test:
	cargo test --tests --examples --all-features

# Run all tests with and without all features (excluding pg16 since github runners do not support it)
test-ci:
	cargo test --tests --examples --features cli
	cargo test --tests --examples --no-default-features

# Run clippy
lint:
	cargo clippy --no-deps --all-targets --all-features -- -W clippy::pedantic \
		-A clippy::let-unit-value \
		-A clippy::wildcard-imports \
		-A clippy::module-name-repetitions \
		-A clippy::uninlined-format-args \
		-A clippy::must-use-candidate \
		-A clippy::doc-markdown \
		-A clippy::missing-panics-doc \
		-A clippy::new-without-default \
		-A clippy::expect-fun-call # this one is particularly bad because I'm just calling format!()

# Generate docs
doc:
	RUSTDOCFLAGS="-Dwarnings" cargo doc --no-deps

# Compute test coverage for CI with llvm-cov
coverage-ci:
	cargo llvm-cov --tests --examples --all-targets --features cli --workspace --lcov --output-path lcov.info

# Compute test coverage with HTML output
coverage:
	cargo llvm-cov --tests --examples --all-targets --all-features --workspace --html
