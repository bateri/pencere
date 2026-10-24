# The image `make dist` builds the Linux executables of the `pencere` command in.
#
# Alpine, so that the C++ of the interpreter (Luau) is compiled against musl by a compiler that
# targets musl: the executables are static, and open on any Linux machine whatever its libc.
# `build-base` is the compiler and the static C++ library; `file` is what says whether a binary
# came out static; `curl` is what `pencere build --target` fetches a runner with, which the tests of the
# command use. The base tag's major.minor is the local rustc's (`make dist` compares them:
# two compilers are two different gates); after `brew upgrade` this line is brought to it.
FROM rust:1.88-alpine

RUN apk add --no-cache build-base file curl

WORKDIR /w
