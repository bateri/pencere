.PHONY: setup fmt check check-heavy audit guest luau dist dist-linux dist-macos dist-sums release-prepare release rehearse publish approve

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
	@for name in LICENSE NOTICE; do for dir in sdk/typescript host/rust wire program kit/view kit/sandbox kit/guest/sdk cli; do cmp -s $$name $$dir/$$name || { echo "check: $$dir/$$name is not the repository's $$name"; exit 1; }; done; done
	@for dir in kit/sandbox kit/guest/sdk; do cmp -s kit/wit/view.wit $$dir/wit/view.wit || { echo "check: $$dir/wit/view.wit is not the interface in kit/wit/view.wit (a published crate cannot read outside itself, so it carries a copy: cp kit/wit/view.wit $$dir/wit/view.wit)"; exit 1; }; done
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
# the engine knows nothing of the wire or of a terminal, the sandbox knows the engine, the wire
# (for the size of a message) and the runtime, and the page modules know the SDK and the engine.
audit:
	@fail=0; \
	if [ "$$(cargo tree -p pencere-view -e normal --depth 1 --prefix none | tail -n +2 | sed 's/ .*//' | sort -u)" != "taffy" ]; then echo "audit: pencere-view depends on something besides the layout crate (taffy) — the page engine depends on its layout crate alone"; fail=1; fi; \
	if cargo tree -p pencere-view -e normal --prefix none | tail -n +2 | grep -E "^(pencere-|containers-|objc2|dispatch2|block2|metal|wgpu|winit)"; then echo "audit: pencere-view links to a crate of this repository or a platform library"; fail=1; fi; \
	if grep -rnE "objc2|dispatch2|block2|wgpu|winit|pencere_(wire|host|sandbox)" kit/view/src | grep -v ":[[:space:]]*//"; then echo "audit: a platform call or a crate of this repository is named in the source of pencere-view"; fail=1; fi; \
	if cargo tree -p pencere-sandbox -e normal --prefix none | tail -n +2 | grep -E "^(pencere-host|containers-|objc2|dispatch2|block2|metal|wgpu|winit)"; then echo "audit: pencere-sandbox links to a crate of this repository past the page engine and the wire, or to a platform library — the sandbox depends on the engine, the wire (for the size of a message) and the runtime alone"; fail=1; fi; \
	if grep -rnE "objc2|dispatch2|block2|wgpu|winit|pencere_host" kit/sandbox/src | grep -v ":[[:space:]]*//"; then echo "audit: a platform call or a crate of the terminal's is named in the source of pencere-sandbox"; fail=1; fi; \
	if cargo tree --workspace -e normal -i wasmtime --depth 1 --prefix none | tail -n +2 | grep -vE "^pencere-sandbox "; then echo "audit: the WebAssembly runtime is linked by a crate other than pencere-sandbox — one door to it"; fail=1; fi; \
	for decoder in png zune-jpeg resvg; do \
		if cargo tree --workspace -e normal -i $$decoder --depth 1 --prefix none | tail -n +2 | grep -vE "^(pencere-sandbox|resvg|usvg|tiny-skia|image) "; then echo "audit: the $$decoder decoder is linked by a crate other than pencere-sandbox — the formats and limits of images are the sandbox's, the same in every terminal"; fail=1; fi; \
	done; \
	if cargo tree --workspace -e normal -i image --depth 1 --prefix none | tail -n +2 | grep -vE "^libsui "; then echo "audit: the image crate is linked by something other than libsui — it is there for the icons of a Windows executable, which the command never writes; nothing that shows a picture links it"; fail=1; fi; \
	if cargo tree --workspace -e normal -i libsui --depth 1 --prefix none | tail -n +2 | grep -vE "^pencere-cli "; then echo "audit: libsui is linked by a crate other than pencere-cli — it is how the command writes an app into an executable, and no library has a use for it"; fail=1; fi; \
	if cargo tree -p pencere-cli -e normal --prefix none | tail -n +2 | grep -E "^(pencere-(host|sandbox|view)|wasmtime|taffy|objc2|dispatch2|block2|metal|wgpu|winit) "; then echo "audit: the pencere command links a terminal's crate or a platform library — it is a program that uses the program library, and hosts nothing"; fail=1; fi; \
	if cargo tree -p pencere-sandbox -e normal --prefix none | tail -n +2 | grep -E "^(fontdb|rustybuzz|ttf-parser|harfrust|skrifa|read-fonts|swash|fontconfig|freetype|font-types)"; then echo "audit: pencere-sandbox links to a font or text-shaping library — an SVG draws shapes and no words, and no font is loaded for it"; fail=1; fi; \
	if grep -rnE "std::(fs|net|process)\b|File::open|TcpStream" kit/sandbox/src --exclude="*_tests.rs" | grep -v ":[[:space:]]*//"; then echo "audit: the sandbox reads a file, opens a connection or runs a program — the only door it has to the bundle is Assets, which the host gives"; fail=1; fi; \
	if grep -nE "^pencere-(host|sandbox) *=" kit/guest/*/Cargo.toml kit/guest/Cargo.toml; then echo "audit: a page module depends on a crate of the terminal side — a module is written with the SDK, the engine and the wire alone"; fail=1; fi; \
	if [ "$$(sh kit/luau/inputs.sh)" != "$$(cat kit/sandbox/luau/inputs.sha256 2>/dev/null)" ]; then echo "audit: kit/sandbox/luau/luau.wasm is not built from the sources of kit/luau and the interface in kit/wit/view.wit as they are now — make luau rebuilds it"; fail=1; fi; \
	test $$fail = 0

# The page modules and the example program, with the toolchain that has a WebAssembly standard
# library: Homebrew's rustc has none, so the modules are built with rustup's `stable`
# (`rustup target add wasm32-unknown-unknown --toolchain stable`). Its linker finds its LLVM
# through the toolchain's library directory, which rustup's own proxies put in the environment
# and a PATH that Homebrew's rustc comes first in does not: it is put there here.
GUEST_MODULES = containers broken chatty tints
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
	cargo build --release -p containers-rust
	# A new file, not the old one written over: macOS kills a signed binary whose file changed
	# under its cached signature (a containers-rust still running in a pane, the next test's exec).
	rm -f kit/guest/dist/containers-rust
	cp target/release/containers-rust kit/guest/dist/containers-rust
	@ls -l kit/guest/dist/containers-rust

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

# The executables of the `pencere` command, for the four targets (macOS on Apple silicon and Intel,
# Linux musl on arm64 and x86_64), in dist/ with SHA256SUMS: what a release of the command carries,
# and what `pencere build --target` fetches. `make dist` is the rehearsal of a release, on this
# machine: the Linux ones are built in Docker (tools/linux), the macOS ones here; each is run where
# this machine can run it, and the example app is made with it and run against the stand-in
# terminal (tools/dist.sh is the how). It is not what is released: the release workflow builds the
# files that are, one runner for each system, with `dist-linux`, `dist-macos` and `dist-sums`.
# Needs Docker and the Apple targets of rustup's toolchain (`rustup target add x86_64-apple-darwin`).
DIST_VERSION = $(shell sed -n 's/^version = "\(.*\)"/\1/p' cli/Cargo.toml | head -n 1)
DIST_DOCKERFILE = tools/linux/Dockerfile
DIST_RUST = $(shell sed -n 's/^FROM rust:\([0-9]*\.[0-9]*\)-.*/\1/p' $(DIST_DOCKERFILE))
DIST_TOOLCHAIN ?= stable
DIST_ENV = DIST_VERSION='$(DIST_VERSION)' DIST_TOOLCHAIN='$(DIST_TOOLCHAIN)' DIST_IMAGE=pencere-linux:$(DIST_RUST) \
	DIST_DOCKERFILE='$(DIST_DOCKERFILE)'
dist:
	@local_rust=$$(rustc --version | sed -n 's/^rustc \([0-9]*\.[0-9]*\).*/\1/p'); \
	if [ "$$local_rust" != "$(DIST_RUST)" ]; then \
		echo "dist: local rustc $$local_rust, image rust:$(DIST_RUST) — bring the FROM line of $(DIST_DOCKERFILE) to the local version"; exit 1; fi
	@if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then \
		echo "SKIPPED: Docker is missing or the daemon does not answer — make dist could not run"; exit 78; fi
	$(DIST_ENV) tools/dist.sh

# The parts of `dist` that the release workflow runs, each on the machine that suits it: one Linux
# executable (ARCH=x86_64 or aarch64) in a container of that architecture, the two macOS ones
# (DIST_TOOLCHAIN names the rustup toolchain; the workflow's is the one the gate runs with), and
# SHA256SUMS once the four are in dist/.
dist-linux:
	@test -n "$(ARCH)" || { echo "usage: make dist-linux ARCH=<x86_64|aarch64>"; exit 2; }
	$(DIST_ENV) tools/dist.sh linux $(ARCH)

dist-macos:
	$(DIST_ENV) tools/dist.sh macos

dist-sums:
	$(DIST_ENV) tools/dist.sh sums

# Releasing a line (RELEASING.md). LINE is one of the lines in tools/lines.mjs: sdk-typescript
# (@pencere/sdk), crates (the six crates, one version) or cli (the pencere command). The same four
# verbs release every line.
#
# release-prepare sets the version(s) and dates the changelog(s); nothing is committed.
release-prepare:
	@test -n "$(LINE)" -a -n "$(VERSION)" || { echo "usage: make release-prepare LINE=<line> VERSION=<x.y.z>"; exit 2; }
	node tools/release.mjs prepare $(LINE) $(VERSION)

# release runs the gate and the release checks, then tags. Nothing is pushed.
release:
	@test -n "$(LINE)" || { echo "usage: make release LINE=<line>"; exit 2; }
	$(MAKE) check
	node tools/release.mjs tag $(LINE)

# rehearse runs the release checks of a prepared line on any branch, without those of the
# repository's state (the branch, a clean tree, origin and its tags). It changes nothing.
rehearse:
	@test -n "$(LINE)" || { echo "usage: make rehearse LINE=<line>"; exit 2; }
	node tools/release.mjs rehearse $(LINE)

# publish pushes main and the tag, and nothing else; the release workflow takes it from there and
# writes a draft. Publishing: run on the maintainer's word.
publish:
	@test -n "$(LINE)" || { echo "usage: make publish LINE=<line>"; exit 2; }
	node tools/release.mjs push $(LINE)

# approve makes the line public everywhere, once the workflow's run is green: the staged package
# on npm (with 2FA), the crates on crates.io, or the verified files of the command; then its GitHub
# release. Publishing: run on the maintainer's word.
approve:
	@test -n "$(LINE)" || { echo "usage: make approve LINE=<line>"; exit 2; }
	node tools/release.mjs approve $(LINE)
