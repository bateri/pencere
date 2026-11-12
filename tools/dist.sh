#!/bin/sh
# The work of `make dist` and of the release workflow's builds (the Makefile says which version and
# which toolchain; this is how). Run from the repository root:
#
#   tools/dist.sh [all]      the whole of it, on a Mac with Docker: the rehearsal of a release
#   tools/dist.sh linux ARCH one Linux executable (x86_64 or aarch64), built in the image of
#                            tools/linux/Dockerfile and tried in it
#   tools/dist.sh macos      the two macOS executables, cross-built by one compiler, tried where this
#                            Mac can run them
#   tools/dist.sh sums       SHA256SUMS, once all eight executables (the command and
#                            the runner of tests, for four targets) are there
#   tools/dist.sh runner TARGET [OUT] [no-terminal]
#                            the runner of one target into OUT (target/runners/pencere-TARGET),
#                            for `pencere build --target TARGET --runner OUT`; `no-terminal` makes
#                            the runner without the terminal in it (the second kind of app,
#                            `pencere build --no-terminal`), into target/runners/pencere-no-terminal-TARGET
#
# The release workflow runs `linux` on a runner of each architecture (DIST_NATIVE=1 refuses a
# machine that is not of that architecture: no emulation) and `macos` on a Mac, gathers the files
# and runs `sums`; `all` is the same on one machine, where the Linux executable of the other
# architecture runs emulated, slow the first time and exact.
#
# The executables are the runner `pencere build` copies for a target, so they are named for the
# release that serves them: pencere-<version>-<target>. The runner is built with the `dist` profile
# of Cargo.toml (the release's, with the whole program optimised as one unit) and has its terminal in
# it, which is what `pencere build` makes an app of unless it is told not to; the one without (the
# kind `pencere build --no-terminal` copies, pencere-no-terminal-<version>-<target>) is built by
# `runner`, and is not one of the eight files: its release is a matter of its own. dist/ is rebuilt from nothing by `all` and
# by `linux` and `macos` on their own, and SHA256SUMS is written last and only when all eight
# executables are there: a dist/ without SHA256SUMS is a build that did not finish.
#
# Linux: static musl executables. The executable makes the example app in the container, and the
# app is run against the stand-in terminal, which is what a person who downloads it does first.
# macOS: both targets cross-built by one compiler. They carry the ad hoc signature the linker
# gives; `pencere build` signs what it makes with its own.
#
# `runner` is for an app built with a pencere that has no release, a commit of it: `pencere build
# --target` fetches the runner of another target from the release of its own version, which is not
# this pencere. The runner is built and checked as an executable of a release is, and nothing else
# of a release is made: not dist/, not the example app, and not the TypeScript half, which only
# `pencere new --page web` carries (a runner with an app in it runs the app and nothing else of the
# command, so the one it has says the SDK was not built).
set -eu

: "${DIST_VERSION:?}"
LINUX_TARGETS="aarch64-unknown-linux-musl x86_64-unknown-linux-musl"
MACOS_TARGETS="aarch64-apple-darwin x86_64-apple-darwin"

fail() {
    echo "dist: $*" >&2
    exit 1
}

name_of() {
    printf 'pencere-%s-%s' "$DIST_VERSION" "$1"
}

# The runner of tests (pencere-test), built beside the command for the same targets: it links the
# kit, which the command that is copied into every app must not, so it is a file of its own.
test_name_of() {
    printf 'pencere-test-%s-%s' "$DIST_VERSION" "$1"
}

hash_of() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

# `pencere new --page web` carries the SDK's page module, which the build of the command takes from
# the SDK's build: the executables must have the real one. Then dist/ is made new.
start() {
    (cd "$(dirname "$0")/.." && npx tsc -b) || fail "the TypeScript half does not build (npx tsc -b); \`make setup\` installs its dependencies"
    rm -rf dist
    mkdir dist
    cp LICENSE NOTICE dist/
}

# ---- Linux ---------------------------------------------------------------------------------

# The container platform of an architecture, what `file` says of an executable of it, and the
# image the executables of it are built in.
linux_platform() {
    case $1 in
        x86_64) platform=linux/amd64 want='x86-64' ;;
        aarch64) platform=linux/arm64 want='ARM aarch64' ;;
        *) fail "no container platform for architecture '$1' (x86_64 or aarch64)" ;;
    esac
    image="$DIST_IMAGE-$1"
}

# `cargo build --locked ARGS --target <ARCH's>`, in the image of tools/linux/Dockerfile, with the
# directory of the target the architecture's own.
linux_cargo() {
    arch=$1
    shift
    : "${DIST_IMAGE:?}" "${DIST_DOCKERFILE:?}"
    linux_platform "$arch"
    target="$arch-unknown-linux-musl"
    if [ "${DIST_NATIVE:-}" = 1 ] && [ "$(uname -m)" != "$arch" ]; then
        fail "this machine is $(uname -m), and the $arch executable is built on a machine of its own architecture"
    fi
    command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 ||
        fail "Docker is missing or the daemon does not answer"
    echo "==> $target, in a $platform container: cargo build $*"
    docker build -q --platform "$platform" -t "$image" -f "$DIST_DOCKERFILE" tools/linux >/dev/null
    # Each architecture has a target directory of its own: the build scripts of one cannot run in
    # the other. The directory is made here, before the container: one the container makes is
    # root's, and on a Linux machine this user then cannot write beside it (a later run).
    mkdir -p "target/dist-$arch"
    docker run --rm --platform "$platform" \
        -v "$PWD":/w -v pencere-linux-cargo:/usr/local/cargo/registry \
        -e CARGO_TARGET_DIR="/w/target/dist-$arch" \
        -e RUSTFLAGS="-C target-feature=+crt-static" "$image" \
        cargo build --locked "$@" --target "$target"
}

# FILE, an executable for ARCH, is checked in the image: static, of that architecture, and saying
# `NAME VERSION` when asked its version.
linux_check() {
    arch=$1
    file=$2
    says=$3
    linux_platform "$arch"
    held=$(cd "$(dirname "$file")" && pwd)
    base=$(basename "$file")
    described=$(docker run --rm --platform "$platform" -v "$held":/out:ro "$image" file "/out/$base")
    echo "$described"
    case $described in
        *"statically linked"* | *"static-pie linked"*) ;;
        *) fail "$file is not statically linked" ;;
    esac
    case $described in
        *"$want"*) ;;
        *) fail "$file is not a $arch executable" ;;
    esac
    shown=$(docker run --rm --platform "$platform" -v "$held":/out:ro "$image" "/out/$base" --version)
    echo "$shown"
    [ "$shown" = "$says" ] || fail "$file says '$shown', not '$says'"
}

# The command for Linux on ARCH, built with the `dist` profile and copied to OUT: with its terminal
# in it, or (KIND no-terminal) without; then checked in the image. It is built by itself: the
# features of the kit's crates are those of everything cargo is asked to build in one run, and a
# program built beside the runner of tests, which links the kit with its code generator, would have
# the code generator too.
linux_build() {
    arch=$1
    out=$2
    kind=${3:-terminal}
    case $kind in
        terminal) set -- ;;
        no-terminal) set -- --no-default-features ;;
        *) fail "the kinds of runner are terminal and no-terminal, not '$kind'" ;;
    esac
    linux_cargo "$arch" --profile dist -p pencere-cli "$@"
    cp "target/dist-$arch/$target/dist/pencere" "$out"
    linux_check "$arch" "$out" "pencere $DIST_VERSION"
}

# The runner of tests (-p pencere-test) and the stand-in terminal, built with the release profile, the
# runner copied into DIR by its name in the release and checked.
linux_tests() {
    arch=$1
    dir=$2
    linux_cargo "$arch" --release -p pencere-fake-host -p pencere-test
    test_out="$dir/$(test_name_of "$target")"
    cp "target/dist-$arch/$target/release/pencere-test" "$test_out"
    linux_check "$arch" "$test_out" "pencere-test $DIST_VERSION"
}

linux() {
    arch=$1
    target="$arch-unknown-linux-musl"
    name=$(name_of "$target")
    linux_build "$arch" "dist/$name"
    # The runner of tests, and the stand-in terminal to try the command against.
    linux_tests "$arch" dist

    # What a person does with it: makes an app of the example with this executable, and runs the
    # app in the pane of the stand-in terminal.
    fake="target/dist-$arch/$target/release/pencere-fake-host"
    check="target/dist-check-$arch"
    rm -rf "$check"
    mkdir -p "$check"
    cp "$fake" "$check/pencere-fake-host"
    docker run --rm --platform "$platform" \
        -v "$PWD/dist":/dist:ro -v "$PWD/examples/containers-lua":/app:ro -v "$PWD/$check":/check "$image" \
        sh -c '/dist/'"$name"' build /app -o /check/made-in-the-container && /check/pencere-fake-host --subscribe stats --call list --then interrupt -- /check/made-in-the-container' \
        >"$check/container.out" 2>"$check/container.err" || {
        cat "$check/container.err"
        fail "the app the container made with dist/$name did not run against the stand-in terminal"
    }
    grep -q 'view: -> call id=2 list' "$check/container.out" "$check/container.err" ||
        { cat "$check/container.err"; fail "the app made in the container did not answer its call"; }
    grep -q 'program: exited with status 130' "$check/container.err" ||
        { cat "$check/container.err"; fail "the app made in the container did not end at the interrupt"; }
    echo "    the app it made runs against the stand-in terminal"
}

# ---- macOS ---------------------------------------------------------------------------------

# The toolchain the macOS executables are built with (DIST_TOOLCHAIN), on Apple silicon, with the
# standard library of each target named.
macos_toolchain() {
    : "${DIST_TOOLCHAIN:?}"
    [ "$(uname -s)" = Darwin ] || fail "the macOS executables are built on a Mac"
    [ "$(uname -m)" = arm64 ] || fail "the macOS executables are built on Apple silicon"
    command -v rustup >/dev/null 2>&1 || fail "rustup is missing (the macOS targets need its toolchain: https://rustup.rs)"
    installed=$(rustup target list --installed --toolchain "$DIST_TOOLCHAIN") ||
        fail "rustup has no toolchain '$DIST_TOOLCHAIN'"
    cargo=$(rustup which cargo --toolchain "$DIST_TOOLCHAIN")
    rustc=$(rustup which rustc --toolchain "$DIST_TOOLCHAIN")
    for target in "$@"; do
        printf '%s\n' "$installed" | grep -qx "$target" ||
            fail "the '$DIST_TOOLCHAIN' toolchain has no standard library for $target; run: rustup target add $target --toolchain $DIST_TOOLCHAIN"
    done
}

# The command for TARGET, built with the `dist` profile and copied to OUT: with its terminal in it,
# or (KIND no-terminal) without. After macos_toolchain. It is built by itself, and the runner of
# tests beside it only when it is the command with its terminal (`macos_tests`), for the reason
# `linux_build` gives.
macos_build() {
    echo "==> $1"
    case ${3:-terminal} in
        terminal) set -- "$1" "$2" ;;
        no-terminal) set -- "$1" "$2" --no-default-features ;;
        *) fail "the kinds of runner are terminal and no-terminal, not '$3'" ;;
    esac
    # The toolchain's own compiler, named: `rustup run` leaves a Homebrew rustc that is earlier
    # on PATH in front of it, and that one has no Apple targets but its own.
    target=$1
    out=$2
    shift 2
    RUSTC="$rustc" "$cargo" build --profile dist --locked -p pencere-cli "$@" --target "$target"
    cp "target/$target/dist/pencere" "$out"
}

# The runner of tests for TARGET, built with the release profile, beside OUT (the command's file).
macos_tests() {
    RUSTC="$rustc" "$cargo" build --release --locked -p pencere-test --target "$1"
    cp "target/$1/release/pencere-test" "$(dirname "$2")/$(test_name_of "$1")"
}

# What this Mac can run of OUT, the executable for TARGET, is run: its own, and the Intel one if
# Rosetta is there. The log says which it was; a Mac without Rosetta does not fail the build. The
# runner of tests beside it, if there is one (`tests`), is run the same way.
macos_try() {
    case $1 in
        aarch64-apple-darwin)
            shown=$("$2" --version)
            [ "$shown" = "pencere $DIST_VERSION" ] || fail "$2 says '$shown', not version $DIST_VERSION"
            codesign --verify "$2" || fail "$2 does not pass codesign --verify"
            echo "    the Apple silicon executable ran and passes codesign --verify"
            if [ "${3:-}" = tests ]; then
                tested="$(dirname "$2")/$(test_name_of "$1")"
                shown=$("$tested" --version)
                [ "$shown" = "pencere-test $DIST_VERSION" ] || fail "$tested says '$shown', not version $DIST_VERSION"
                echo "    the runner of tests ran"
            fi
            ;;
        x86_64-apple-darwin)
            if shown=$(arch -x86_64 "$2" --version 2>/dev/null); then
                [ "$shown" = "pencere $DIST_VERSION" ] || fail "the Intel Mac executable says '$shown'"
                if [ "${3:-}" = tests ]; then
                    tested="$(dirname "$2")/$(test_name_of "$1")"
                    shown=$(arch -x86_64 "$tested" --version 2>/dev/null) || fail "the runner of tests for the Intel Mac did not run"
                    [ "$shown" = "pencere-test $DIST_VERSION" ] || fail "the Intel Mac runner of tests says '$shown'"
                fi
                echo "    the Intel executable ran (under Rosetta)"
            else
                echo "dist: the Intel Mac executable was not run (this Mac cannot run it)"
            fi
            ;;
    esac
}

macos() {
    macos_toolchain $MACOS_TARGETS
    for target in $MACOS_TARGETS; do
        macos_build "$target" "dist/$(name_of "$target")"
        macos_tests "$target" "dist/$(name_of "$target")"
    done
    for target in $MACOS_TARGETS; do
        macos_try "$target" "dist/$(name_of "$target")" tests
    done
}

# The Linux executables are made into the app on this Mac without their toolchains, and run: the
# app this Mac makes with the Linux executable is a Linux executable. Needs both halves above.
cross() {
    own=aarch64-apple-darwin
    for target in $LINUX_TARGETS; do
        arch=${target%%-*}
        case $arch in
            x86_64) platform=linux/amd64 ;;
            aarch64) platform=linux/arm64 ;;
        esac
        image="$DIST_IMAGE-$arch"
        check="target/dist-check-$arch"
        runner="dist/$(name_of "$target")"
        "dist/$(name_of "$own")" build examples/containers-lua --runner "$runner" --target "$target" -o "$check/made-on-the-mac"
        docker run --rm --platform "$platform" -v "$PWD/$check":/check "$image" \
            sh -c '/check/pencere-fake-host --subscribe stats --call list --then interrupt -- /check/made-on-the-mac' \
            >"$check/mac.out" 2>"$check/mac.err" || {
            cat "$check/mac.err"
            fail "the app this Mac made with $runner did not run against the stand-in terminal"
        }
        grep -q 'program: exited with status 130' "$check/mac.err" ||
            { cat "$check/mac.err"; fail "the app this Mac made for $target did not end at the interrupt"; }
        echo "    the app this Mac made for $target runs against the stand-in terminal"
    done
}

# ---- the sums ------------------------------------------------------------------------------

sums() {
    (
        cd dist
        for target in $LINUX_TARGETS $MACOS_TARGETS; do
            [ -s "$(name_of "$target")" ] || fail "dist/$(name_of "$target") is missing"
            [ -s "$(test_name_of "$target")" ] || fail "dist/$(test_name_of "$target") is missing"
        done
        hash_of "$(name_of aarch64-apple-darwin)" "$(name_of x86_64-apple-darwin)" \
            "$(name_of aarch64-unknown-linux-musl)" "$(name_of x86_64-unknown-linux-musl)" \
            "$(test_name_of aarch64-apple-darwin)" "$(test_name_of x86_64-apple-darwin)" \
            "$(test_name_of aarch64-unknown-linux-musl)" "$(test_name_of x86_64-unknown-linux-musl)" >SHA256SUMS
    )
    ls -l dist
}

# ---- one runner ----------------------------------------------------------------------------

runner() {
    target=$1
    kind=${3:-terminal}
    case $kind in
        terminal) out=${2:-target/runners/pencere-$target} ;;
        no-terminal) out=${2:-target/runners/pencere-no-terminal-$target} ;;
        *) fail "the kinds of runner are terminal and no-terminal, not '$kind'" ;;
    esac
    mkdir -p "$(dirname "$out")"
    case $target in
        *-unknown-linux-musl) linux_build "${target%%-*}" "$out" "$kind" ;;
        aarch64-apple-darwin | x86_64-apple-darwin)
            macos_toolchain "$target"
            macos_build "$target" "$out" "$kind"
            macos_try "$target" "$out"
            ;;
        *) fail "no runner for '$target': the targets are $LINUX_TARGETS $MACOS_TARGETS" ;;
    esac
    echo "runner: $out"
}

case ${1:-all} in
    all)
        [ "$(uname -s)" = Darwin ] || fail "the whole of it is made on a Mac"
        start
        for target in $LINUX_TARGETS; do linux "${target%%-*}"; done
        macos
        cross
        sums
        ;;
    linux)
        start
        linux "${2:?usage: tools/dist.sh linux x86_64|aarch64}"
        ;;
    macos)
        start
        macos
        ;;
    sums)
        sums
        ;;
    runner)
        runner "${2:?usage: tools/dist.sh runner TARGET [OUT] [no-terminal]}" "${3:-}" "${4:-}"
        ;;
    *)
        fail "usage: tools/dist.sh [all | linux ARCH | macos | sums | runner TARGET [OUT] [no-terminal]]"
        ;;
esac
