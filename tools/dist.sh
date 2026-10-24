#!/bin/sh
# The work of `make dist` (the Makefile says which version and which toolchain; this is how). Run
# from the repository root.
#
# dist/ is rebuilt from nothing and SHA256SUMS is written last, only when all four executables are
# there and each has been looked at, and run where this machine can run it: a dist/ without
# SHA256SUMS is a build that did not finish. The executables are the runner `pencere build` copies
# for a target, so they are named for the release that serves them: pencere-<version>-<target>.
#
# Linux: static musl executables, built in a container of their own architecture (the other one
# than the machine's runs emulated, slow the first time and exact), and then used in it: the
# executable makes the example app and the app is run against the stand-in terminal, which
# is what a person who downloads it does first.
# macOS: both targets cross-built by one compiler. They carry the ad hoc signature the linker
# gives; `pencere build` signs what it makes with its own.
set -eu

: "${DIST_VERSION:?}" "${DIST_TOOLCHAIN:?}" "${DIST_IMAGE:?}" "${DIST_DOCKERFILE:?}"
LINUX_TARGETS="aarch64-unknown-linux-musl x86_64-unknown-linux-musl"
MACOS_TARGETS="aarch64-apple-darwin x86_64-apple-darwin"

fail() {
    echo "dist: $*" >&2
    exit 1
}

[ "$(uname -s)" = Darwin ] || fail "the macOS executables are built on a Mac"
command -v rustup >/dev/null 2>&1 || fail "rustup is missing (the macOS targets need its toolchain: https://rustup.rs)"
installed=$(rustup target list --installed --toolchain "$DIST_TOOLCHAIN") ||
    fail "rustup has no toolchain '$DIST_TOOLCHAIN'"
cargo=$(rustup which cargo --toolchain "$DIST_TOOLCHAIN")
rustc=$(rustup which rustc --toolchain "$DIST_TOOLCHAIN")
for target in $MACOS_TARGETS; do
    printf '%s\n' "$installed" | grep -qx "$target" ||
        fail "the '$DIST_TOOLCHAIN' toolchain has no standard library for $target; run: rustup target add $target --toolchain $DIST_TOOLCHAIN"
done

# `pencere new --page web` carries the SDK's page module, which the build of the command takes from
# the SDK's build: the executables must have the real one.
(cd "$(dirname "$0")/.." && npx tsc -b) || fail "the TypeScript half does not build (npx tsc -b); \`make setup\` installs its dependencies"

rm -rf dist
mkdir dist
cp LICENSE NOTICE dist/

name_of() {
    printf 'pencere-%s-%s' "$DIST_VERSION" "$1"
}

# ---- Linux ---------------------------------------------------------------------------------

for target in $LINUX_TARGETS; do
    arch=${target%%-*}
    case $arch in
        x86_64) platform=linux/amd64 want='x86-64' ;;
        aarch64) platform=linux/arm64 want='ARM aarch64' ;;
        *) fail "no container platform for $target" ;;
    esac
    image="$DIST_IMAGE-$arch"
    name=$(name_of "$target")
    echo "==> $target, in a $platform container"
    docker build -q --platform "$platform" -t "$image" -f "$DIST_DOCKERFILE" tools/linux >/dev/null
    # Each architecture has a target directory of its own: the build scripts of one cannot run in
    # the other. The stand-in terminal is built with the command, to try it against.
    docker run --rm --platform "$platform" \
        -v "$PWD":/w -v pencere-linux-cargo:/usr/local/cargo/registry \
        -e CARGO_TARGET_DIR="/w/target/dist-$arch" \
        -e RUSTFLAGS="-C target-feature=+crt-static" "$image" \
        cargo build --release --locked -p pencere-cli -p pencere-fake-host --target "$target"
    cp "target/dist-$arch/$target/release/pencere" "dist/$name"

    described=$(docker run --rm --platform "$platform" -v "$PWD/dist":/dist:ro "$image" file "/dist/$name")
    echo "$described"
    case $described in
        *"statically linked"* | *"static-pie linked"*) ;;
        *) fail "dist/$name is not statically linked" ;;
    esac
    case $described in
        *"$want"*) ;;
        *) fail "dist/$name is not a $arch executable" ;;
    esac
    shown=$(docker run --rm --platform "$platform" -v "$PWD/dist":/dist:ro "$image" "/dist/$name" --version)
    echo "$shown"
    [ "$shown" = "pencere $DIST_VERSION" ] || fail "dist/$name says '$shown', not version $DIST_VERSION"

    # What a person does with it: makes an app of the example with this executable, and runs the
    # app in the pane of the stand-in terminal; and runs the app this Mac makes of it with the
    # same executable (nothing of the target's toolchain is on this machine).
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
done

# ---- macOS ---------------------------------------------------------------------------------

for target in $MACOS_TARGETS; do
    echo "==> $target"
    # The toolchain's own compiler, named: `rustup run` leaves a Homebrew rustc that is earlier on
    # PATH in front of it, and that one has no Apple targets but its own.
    RUSTC="$rustc" "$cargo" build --release --locked -p pencere-cli --target "$target"
    cp "target/$target/release/pencere" "dist/$(name_of "$target")"
done

# What this Mac can run is run: its own executable, and the other Mac one if Rosetta is there.
own=aarch64-apple-darwin
shown=$("dist/$(name_of "$own")" --version)
[ "$shown" = "pencere $DIST_VERSION" ] || fail "dist/$(name_of "$own") says '$shown', not version $DIST_VERSION"
codesign --verify "dist/$(name_of "$own")" || fail "dist/$(name_of "$own") does not pass codesign --verify"
if shown=$(arch -x86_64 "dist/$(name_of x86_64-apple-darwin)" --version 2>/dev/null); then
    [ "$shown" = "pencere $DIST_VERSION" ] || fail "the Intel Mac executable says '$shown'"
else
    echo "dist: the Intel Mac executable was not run (no Rosetta here)"
fi

# The other targets' executables, made into the app on this Mac without their toolchains, and the
# Linux one run: the app this Mac makes with the Linux executable is a Linux executable.
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

# ---- the sums ------------------------------------------------------------------------------

(
    cd dist
    for target in $LINUX_TARGETS $MACOS_TARGETS; do
        [ -s "$(name_of "$target")" ] || fail "dist/$(name_of "$target") is missing"
    done
    shasum -a 256 "$(name_of aarch64-apple-darwin)" "$(name_of x86_64-apple-darwin)" \
        "$(name_of aarch64-unknown-linux-musl)" "$(name_of x86_64-unknown-linux-musl)" >SHA256SUMS
)
ls -l dist
