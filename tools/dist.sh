#!/bin/sh
# The work of `make dist` and of the release workflow's builds (the Makefile says which version and
# which toolchain; this is how). Run from the repository root:
#
#   tools/dist.sh [all]      the whole of it, on a Mac with Docker: the rehearsal of a release
#   tools/dist.sh linux ARCH one Linux executable (x86_64 or aarch64), built in the image of
#                            tools/linux/Dockerfile and tried in it
#   tools/dist.sh macos      the two macOS executables, cross-built by one compiler, tried where this
#                            Mac can run them
#   tools/dist.sh sums       SHA256SUMS, once all four executables are there
#   tools/dist.sh runner TARGET [OUT]
#                            the runner of one target into OUT (target/runners/pencere-TARGET),
#                            for `pencere build --target TARGET --runner OUT`
#
# The release workflow runs `linux` on a runner of each architecture (DIST_NATIVE=1 refuses a
# machine that is not of that architecture: no emulation) and `macos` on a Mac, gathers the files
# and runs `sums`; `all` is the same on one machine, where the Linux executable of the other
# architecture runs emulated, slow the first time and exact.
#
# The executables are the runner `pencere build` copies for a target, so they are named for the
# release that serves them: pencere-<version>-<target>. dist/ is rebuilt from nothing by `all` and
# by `linux` and `macos` on their own, and SHA256SUMS is written last and only when all four
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

# The command for Linux on ARCH, built in the image of tools/linux/Dockerfile and copied to OUT,
# with the packages named after it (-p NAME) built beside it; then checked in the image: static, of
# that architecture, and saying this version.
linux_build() {
    arch=$1
    out=$2
    shift 2
    : "${DIST_IMAGE:?}" "${DIST_DOCKERFILE:?}"
    linux_platform "$arch"
    target="$arch-unknown-linux-musl"
    if [ "${DIST_NATIVE:-}" = 1 ] && [ "$(uname -m)" != "$arch" ]; then
        fail "this machine is $(uname -m), and the $arch executable is built on a machine of its own architecture"
    fi
    command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 ||
        fail "Docker is missing or the daemon does not answer"
    echo "==> $target, in a $platform container"
    docker build -q --platform "$platform" -t "$image" -f "$DIST_DOCKERFILE" tools/linux >/dev/null
    # Each architecture has a target directory of its own: the build scripts of one cannot run in
    # the other. The directory is made here, before the container: one the container makes is
    # root's, and on a Linux machine this user then cannot write beside it (a later run).
    mkdir -p "target/dist-$arch"
    docker run --rm --platform "$platform" \
        -v "$PWD":/w -v pencere-linux-cargo:/usr/local/cargo/registry \
        -e CARGO_TARGET_DIR="/w/target/dist-$arch" \
        -e RUSTFLAGS="-C target-feature=+crt-static" "$image" \
        cargo build --release --locked -p pencere-cli "$@" --target "$target"
    cp "target/dist-$arch/$target/release/pencere" "$out"

    held=$(cd "$(dirname "$out")" && pwd)
    file=$(basename "$out")
    described=$(docker run --rm --platform "$platform" -v "$held":/out:ro "$image" file "/out/$file")
    echo "$described"
    case $described in
        *"statically linked"* | *"static-pie linked"*) ;;
        *) fail "$out is not statically linked" ;;
    esac
    case $described in
        *"$want"*) ;;
        *) fail "$out is not a $arch executable" ;;
    esac
    shown=$(docker run --rm --platform "$platform" -v "$held":/out:ro "$image" "/out/$file" --version)
    echo "$shown"
    [ "$shown" = "pencere $DIST_VERSION" ] || fail "$out says '$shown', not version $DIST_VERSION"
}

linux() {
    arch=$1
    target="$arch-unknown-linux-musl"
    name=$(name_of "$target")
    # The stand-in terminal is built with the command, to try it against.
    linux_build "$arch" "dist/$name" -p pencere-fake-host

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

# The command for TARGET, copied to OUT. After macos_toolchain.
macos_build() {
    echo "==> $1"
    # The toolchain's own compiler, named: `rustup run` leaves a Homebrew rustc that is earlier
    # on PATH in front of it, and that one has no Apple targets but its own.
    RUSTC="$rustc" "$cargo" build --release --locked -p pencere-cli --target "$1"
    cp "target/$1/release/pencere" "$2"
}

# What this Mac can run of OUT, the executable for TARGET, is run: its own, and the Intel one if
# Rosetta is there. The log says which it was; a Mac without Rosetta does not fail the build.
macos_try() {
    case $1 in
        aarch64-apple-darwin)
            shown=$("$2" --version)
            [ "$shown" = "pencere $DIST_VERSION" ] || fail "$2 says '$shown', not version $DIST_VERSION"
            codesign --verify "$2" || fail "$2 does not pass codesign --verify"
            echo "    the Apple silicon executable ran and passes codesign --verify"
            ;;
        x86_64-apple-darwin)
            if shown=$(arch -x86_64 "$2" --version 2>/dev/null); then
                [ "$shown" = "pencere $DIST_VERSION" ] || fail "the Intel Mac executable says '$shown'"
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
    done
    for target in $MACOS_TARGETS; do
        macos_try "$target" "dist/$(name_of "$target")"
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
        done
        hash_of "$(name_of aarch64-apple-darwin)" "$(name_of x86_64-apple-darwin)" \
            "$(name_of aarch64-unknown-linux-musl)" "$(name_of x86_64-unknown-linux-musl)" >SHA256SUMS
    )
    ls -l dist
}

# ---- one runner ----------------------------------------------------------------------------

runner() {
    target=$1
    out=${2:-target/runners/pencere-$target}
    mkdir -p "$(dirname "$out")"
    case $target in
        *-unknown-linux-musl) linux_build "${target%%-*}" "$out" ;;
        aarch64-apple-darwin | x86_64-apple-darwin)
            macos_toolchain "$target"
            macos_build "$target" "$out"
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
        runner "${2:?usage: tools/dist.sh runner TARGET [OUT]}" ${3:+"$3"}
        ;;
    *)
        fail "usage: tools/dist.sh [all | linux ARCH | macos | sums | runner TARGET [OUT]]"
        ;;
esac
