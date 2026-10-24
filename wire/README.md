# pencere-wire

The half of the [pencere](https://github.com/bateri/pencere) wire that a program and a terminal
share: the APC frames, the payloads of the handshake and of closing, bundle manifests with their
digests and the verification of the files that arrive, and the six messages of a program and its
view. A terminal reads it with the host crate (`pencere-host`); a program writes it with the
program library. Neither has a second copy of it.

The crate does no I/O. Every byte it reads comes from a stream anyone can write to: it never
panics on it and never buffers more than a limit of the specification allows.

The wire is version 1 of the
[specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md), still a
draft. [CHANGELOG.md](CHANGELOG.md) says what each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
