.PHONY: setup fmt check

# Installs the TypeScript half's dependencies from the lockfile. The only
# target that reaches the network.
setup:
	npm ci

fmt:
	cargo fmt --all
	npx prettier --write .

# The gate: both halves, formatted, linted, type-checked and tested, and the
# specification's examples checked against its fixtures. The TypeScript half is
# built before the Rust tests run, because the end-to-end tests start the example
# tool from its build. It does not install anything, so it never reaches the
# network.
check:
	@test -d node_modules || { echo "check: node_modules is missing, run 'make setup' first"; exit 1; }
	node tools/check-spec.mjs
	npx tsc -b
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	npx prettier --check .
	npx vitest run --passWithNoTests
