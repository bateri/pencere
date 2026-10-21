.PHONY: setup fmt check release-prepare release publish publish-local

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
	@for name in LICENSE NOTICE; do for dir in sdk/typescript host/rust cli create; do cmp -s $$name $$dir/$$name || { echo "check: $$dir/$$name is not the repository's $$name"; exit 1; }; done; done
	npx tsc -b
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	npx prettier --check .
	npx vitest run --passWithNoTests

# Releasing a package (RELEASING.md). LINE is one of the lines in tools/release.mjs:
# sdk-typescript (@pencere/sdk) or host-rust (the pencere crate).
#
# release-prepare sets the version and dates the changelog; nothing is committed.
release-prepare:
	@test -n "$(LINE)" -a -n "$(VERSION)" || { echo "usage: make release-prepare LINE=<line> VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(LINE) $(VERSION)

# release runs the gate and the release checks, then tags. Nothing is pushed.
release:
	@test -n "$(LINE)" || { echo "usage: make release LINE=<line>"; exit 2; }
	$(MAKE) check
	node tools/release.mjs tag $(LINE)

# publish pushes main and the tag; the release workflow stages the package and a
# maintainer approves it on npm with 2FA. Publishing: run on the maintainer's word.
publish:
	@test -n "$(LINE)" || { echo "usage: make publish LINE=<line>"; exit 2; }
	node tools/release.mjs push $(LINE)

# publish-local publishes from this machine, then pushes: a line's first release,
# before the registry knows the package and its trusted publisher.
publish-local:
	@test -n "$(LINE)" || { echo "usage: make publish-local LINE=<line>"; exit 2; }
	node tools/release.mjs local $(LINE)
