.PHONY: setup fmt check check-heavy audit guest luau release-prepare release publish publish-local approve

# Installs the TypeScript half's dependencies from the lockfile. The only
# target that reaches the network.
setup:
	npm ci

fmt:
	cargo fmt --all
	cargo fmt --manifest-path kit/guest/Cargo.toml --all
	npx prettier --write .

# The gate: both halves, formatted, linted, type-checked and tested, and the
# specification's examples checked against its fixtures. The TypeScript half is
# built before the Rust tests run, because the stand-in host's tests start programs
# that import `@pencere/sdk`, which they run from its build. It does not install
# anything, so it never reaches the network, and it needs no toolchain for WebAssembly:
# the sandbox's tests that run a real module or the interpreter are `check-heavy`'s.
check:
	@test -d node_modules || { echo "check: node_modules is missing, run 'make setup' first"; exit 1; }
	node tools/check-spec.mjs
	@for name in LICENSE NOTICE; do for dir in sdk/typescript host/rust wire kit/view kit/sandbox; do cmp -s $$name $$dir/$$name || { echo "check: $$dir/$$name is not the repository's $$name"; exit 1; }; done; done
	@diff -r kit/luau/licenses kit/sandbox/licenses >/dev/null || { echo "check: kit/sandbox/licenses is not a copy of kit/luau/licenses (the crate carries the licences of what is built into its interpreter)"; exit 1; }
	$(MAKE) --no-print-directory audit
	npx tsc -b
	cargo fmt --all -- --check
	cargo fmt --manifest-path kit/guest/Cargo.toml --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	npx prettier --check .
	npx vitest run --passWithNoTests

# The rules of the kit that are not the compiler's: what each crate may depend on, and that the
# interpreter in the repository is the one its sources build. They are the layers of the kit:
# the engine knows nothing of the wire or of a terminal, the sandbox knows the engine and the
# runtime, and the page modules know the SDK and the engine.
audit:
	@fail=0; \
	if [ "$$(cargo tree -p pencere-view -e normal --depth 1 --prefix none | tail -n +2 | sed 's/ .*//' | sort -u)" != "taffy" ]; then echo "audit: pencere-view depends on something besides the layout crate (taffy) — the page engine depends on its layout crate alone"; fail=1; fi; \
	if cargo tree -p pencere-view -e normal --prefix none | tail -n +2 | grep -E "^(pencere-|containers-|objc2|dispatch2|block2|metal|wgpu|winit)"; then echo "audit: pencere-view links to a crate of this repository or a platform library"; fail=1; fi; \
	if grep -rnE "objc2|dispatch2|block2|wgpu|winit|pencere_(wire|host|sandbox)" kit/view/src | grep -v ":[[:space:]]*//"; then echo "audit: a platform call or a crate of this repository is named in the source of pencere-view"; fail=1; fi; \
	if cargo tree -p pencere-sandbox -e normal --prefix none | tail -n +2 | grep -E "^(pencere-(wire|host)|containers-|objc2|dispatch2|block2|metal|wgpu|winit)"; then echo "audit: pencere-sandbox links to a crate of this repository past the page engine or to a platform library — the sandbox depends on the engine and the runtime alone"; fail=1; fi; \
	if grep -rnE "objc2|dispatch2|block2|wgpu|winit|pencere_(wire|host)" kit/sandbox/src | grep -v ":[[:space:]]*//"; then echo "audit: a platform call or a crate of this repository is named in the source of pencere-sandbox"; fail=1; fi; \
	if cargo tree --workspace -e normal -i wasmtime --depth 1 --prefix none | tail -n +2 | grep -vE "^pencere-sandbox "; then echo "audit: the WebAssembly runtime is linked by a crate other than pencere-sandbox — one door to it"; fail=1; fi; \
	if grep -nE "^pencere-(host|sandbox) *=" kit/guest/*/Cargo.toml kit/guest/Cargo.toml; then echo "audit: a page module depends on a crate of the terminal side — a module is written with the SDK, the engine and the wire alone"; fail=1; fi; \
	if [ "$$(sh kit/luau/inputs.sh)" != "$$(cat kit/sandbox/luau/inputs.sha256 2>/dev/null)" ]; then echo "audit: kit/sandbox/luau/luau.wasm is not built from the sources of kit/luau and the interface in kit/wit/view.wit as they are now — make luau rebuilds it"; fail=1; fi; \
	test $$fail = 0

# The page modules and the example program, with the toolchain that has a WebAssembly standard
# library: Homebrew's rustc has none, so the modules are built with rustup's `stable`
# (`rustup target add wasm32-unknown-unknown --toolchain stable`). Its linker finds its LLVM
# through the toolchain's library directory, which rustup's own proxies put in the environment
# and a PATH that Homebrew's rustc comes first in does not: it is put there here.
GUEST_MODULES = containers broken chatty
GUEST_BIN = $(shell dirname "$$(rustup which --toolchain stable cargo 2>/dev/null)" 2>/dev/null)
GUEST_CARGO = PATH="$(GUEST_BIN):$$PATH" DYLD_FALLBACK_LIBRARY_PATH="$(dir $(GUEST_BIN))lib" $(GUEST_BIN)/cargo
guest:
	@command -v rustup >/dev/null 2>&1 || { echo "guest: rustup is missing (the modules are built with its stable toolchain)"; exit 1; }
	@rustup target list --installed --toolchain stable 2>/dev/null | grep -qx wasm32-unknown-unknown || { echo "guest: rustup's stable toolchain has no wasm32-unknown-unknown target — rustup target add wasm32-unknown-unknown --toolchain stable"; exit 1; }
	$(GUEST_CARGO) build --manifest-path kit/guest/Cargo.toml --release --target wasm32-unknown-unknown $(addprefix -p ,$(GUEST_MODULES))
	mkdir -p kit/guest/dist
	@for m in $(GUEST_MODULES); do \
		$(GUEST_CARGO) run -q --manifest-path kit/guest/Cargo.toml -p componentize -- kit/guest/target/wasm32-unknown-unknown/release/$$m.wasm kit/guest/dist/$$m.wasm || exit 1; \
	done
	@ls -l $(addprefix kit/guest/dist/,$(addsuffix .wasm,$(GUEST_MODULES)))
	# The program carries the container list's module (`containers.wasm`, built just above) in its
	# own file, so it is built after it.
	cargo build --release -p containers-native
	# A new file, not the old one written over: macOS kills a signed binary whose file changed
	# under its cached signature (a containers-native still running in a pane, the next test's exec).
	rm -f kit/guest/dist/containers-native
	cp target/release/containers-native kit/guest/dist/containers-native
	@ls -l kit/guest/dist/containers-native

# The slow half of the gate: what runs a real module, the interpreter or a toolchain for
# WebAssembly. It builds the modules first (`guest`), then runs every test `make check` leaves
# out (`#[ignore = "heavy: ..."]`: the sandbox against the real page modules and against the
# interpreter, with the scripts of the example), and lints the page modules for the target they
# run on. Needs rustup with the wasm32 target (see `guest`); `luau` (Docker) is not part of it.
check-heavy: guest
	cargo test --workspace --all-targets -- --ignored
	$(GUEST_CARGO) clippy --manifest-path kit/guest/Cargo.toml --all-targets --target wasm32-unknown-unknown -- -D warnings

# The interpreter of the scripting language (`kit/sandbox/luau/luau.wasm`): Luau, the bridge that
# gives a script the page interface, and the preludes that are the language's side of it
# (`kit/luau`), as a component. It is built in wasi-sdk's own container (the image, the release of
# Luau and its digest are pinned in `kit/luau/pins`; nothing is installed on this machine but
# Docker) and kept in the repository, so that building and `make check` need no toolchain for
# WebAssembly. Whenever the interface (`kit/wit/view.wit`) or any of what the module is built from
# changes, it is built again here — `make check` fails, naming this target, while it is not — and
# the scripts people wrote do not: they run on whatever the interpreter has become.
LUAU_WORK = kit/guest/target/luau
LUAU_OUT = kit/sandbox/luau
luau:
	@command -v rustup >/dev/null 2>&1 || { echo "luau: rustup is missing (the tools that make the component are built with its stable toolchain)"; exit 1; }
	@if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then \
		echo "SKIPPED: Docker is missing or the daemon does not answer — make luau could not run"; exit 78; fi
	@set -e; . ./kit/luau/pins; \
	mkdir -p $(LUAU_WORK); \
	archive=$(LUAU_WORK)/luau-$$LUAU_VERSION.tar.gz; \
	digest() { shasum -a 256 "$$1" | cut -d' ' -f1; }; \
	if [ ! -f "$$archive" ] || [ "$$(digest "$$archive")" != "$$LUAU_SHA256" ]; then \
		curl -fsSL -o "$$archive" "https://github.com/luau-lang/luau/archive/refs/tags/$$LUAU_VERSION.tar.gz"; fi; \
	[ "$$(digest "$$archive")" = "$$LUAU_SHA256" ] || { echo "luau: the archive of Luau $$LUAU_VERSION is not the one pinned in kit/luau/pins (sha256 $$LUAU_SHA256)"; exit 1; }; \
	rm -rf $(LUAU_WORK)/src $(LUAU_WORK)/out $(LUAU_WORK)/bindings; \
	mkdir -p $(LUAU_WORK)/src; \
	tar -xzf "$$archive" -C $(LUAU_WORK)/src --strip-components=1; \
	$(GUEST_CARGO) run -q --manifest-path kit/guest/Cargo.toml -p cbindgen -- kit/wit view $(LUAU_WORK)/bindings; \
	docker run --rm --user "$$(id -u):$$(id -g)" -v "$(CURDIR)/kit/luau":/luau:ro -v "$(CURDIR)/$(LUAU_WORK)":/work \
		"$$WASI_SDK_IMAGE" sh /luau/build.sh /work/src /work/bindings /work/out; \
	$(GUEST_CARGO) run -q --manifest-path kit/guest/Cargo.toml -p componentize -- $(LUAU_WORK)/out/luau-core.wasm $(LUAU_OUT)/luau.wasm; \
	sh kit/luau/inputs.sh >$(LUAU_OUT)/inputs.sha256; \
	ls -l $(LUAU_OUT)/luau.wasm

# Releasing a package (RELEASING.md). LINE is one of the lines in tools/lines.mjs:
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

# approve makes public the version the release workflow staged on npm, with 2FA.
# Publishing: run on the maintainer's word.
approve:
	@test -n "$(LINE)" || { echo "usage: make approve LINE=<line>"; exit 2; }
	node tools/release.mjs approve $(LINE)
