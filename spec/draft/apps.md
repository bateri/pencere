# pencere apps

**The file that releases a program that shows a view** — specification draft

|           |                                                      |
| --------- | ---------------------------------------------------- |
| Status    | Draft                                                |
| Draft of  | 2026-11-07                                           |
| Builds on | `pencere.md`, Sections 5.2, 5.4 and 5.5 (protocol 1) |

## 1. Introduction

A program that shows a view in a terminal pane (`pencere.md`) is released as one executable file per
machine target. This document defines the two small files that go with a release so that a terminal
can follow the program on its person's behalf: find the newest release it can open, check that the
file it downloads is the one the author released, and put it on the machines the person works on.

The files are:

- `app.json`, which names the app and its version, says which protocol versions and which kinds of
  view the release needs, gives the address and SHA-256 digest of the file for each target, and lists
  the earlier releases that a terminal which cannot open this one can fall back to;
- `app.json.sig`, the author's signature over the bytes of `app.json`.

This document defines the contents of those files, how a signature is checked, and how a reader
chooses a release. It does not define where the files are published beyond the names of Section 3,
how a terminal downloads or stores a release, or which authors a terminal trusts (Section 8).

## 2. Conventions

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT",
"RECOMMENDED", "NOT RECOMMENDED", "MAY" and "OPTIONAL" are to be interpreted as described in BCP 14
(RFC 2119, RFC 8174) when, and only when, they appear in all capitals. Grammar is ABNF (RFC 5234);
JSON is RFC 8259. A **reader** is a program that reads these files: usually a terminal, and the
tool an author releases with.

This document is versioned apart from the protocol of `pencere.md`: a change of what it says is a
change of this document and of its fixtures, not of the wire.

The examples in Section 10 are fixtures, byte for byte. A block headed `file <path>` is the text of
the fixture file it names, with a line feed between its lines and none at the end; a tool checks
that they match.

## 3. The two files

A release of an app publishes `app.json` and `app.json.sig`, next to the executable files its
`app.json` names. The names are fixed: a publisher that serves a release's files from an address
that depends on them (GitHub's `releases/latest/download/<file>`) needs the reader to ask for these
names and no others. A repository releases one app; the app's name is in `app.json`, not in the
name of the file.

The address of a signature is the address of its `app.json` with `.sig` appended. A reader that
holds an address of `app.json` (a source it was given, or the `manifest` of a `previous` entry,
Section 4.6) finds the signature there.

An `app.json` is at most 65 536 bytes. A reader MUST NOT read, buffer or parse more than that
before it has rejected the file, however long the stream it comes from.

## 4. `app.json`

A UTF-8 JSON text (RFC 8259) without a byte order mark, whose value is an object with the members
below. All are REQUIRED. A reader ignores any other member, so that a later version of this document
can add one; a writer MUST NOT depend on a reader reading it.

| Member     | Value                                                                               |
| ---------- | ----------------------------------------------------------------------------------- |
| `name`     | the app's name (4.1)                                                                |
| `version`  | this release's version (4.2)                                                        |
| `protocol` | the protocol versions the release speaks, as the `versions` of a `hello` (4.3)      |
| `views`    | the kinds of view the release can show, as the `views` of a `hello` (4.3)           |
| `key`      | the author's public key (5.1)                                                       |
| `files`    | an object from a target name to the executable file for it (4.4)                    |
| `previous` | earlier releases a reader may fall back to (4.6), at most 32 entries, possibly none |

### 4.1 Name

```abnf
name  = start *63( start / "-" )
start = %x61-7A / %x30-39
```

Lowercase letters, digits and `-`, from one to 64 bytes, beginning with a letter or a digit. The name is what the app is called on every machine
the reader puts it on, so a reader MAY refuse a name it reserves for itself.

### 4.2 Version

```abnf
version = number "." number "." number
number  = "0" / ( %x31-39 *8DIGIT )
```

Exactly three numbers, written in decimal without a leading zero, each at most nine digits. No
pre-release or build part. Versions are compared by number, the first number first: `0.10.0` is
newer than `0.9.0`.

### 4.3 Protocol and views

`protocol` is a list of positive integers, at least one: the protocol versions of `pencere.md`
Section 5.4 the program speaks, as it lists them in its `hello`. `views` is a list of entries, at
least one, with the shape of the `views` of a `hello` (Section 5.1, Appendix A): a `kind` string and
a `versions` list of strings, in the order the program prefers them. A kind that is not in the table
of Section 5.5 is kept as written.

A release's **requirement** is its `protocol` and its `views` together. Two requirements are the
same when both are equal as written.

### 4.4 Files

`files` has one member per target the release is built for. The member's name is a **target**, from
one to 64 bytes of lowercase letters, digits, `_` and `-` (`aarch64-apple-darwin` is the usual
spelling); this document does not say which targets exist, and a reader picks the one that names
the machine it is putting the app on. Its value is an object with two members:

- `url`: the address of the executable file, one file and no archive (4.5);
- `sha256`: the SHA-256 digest of that file, 64 lowercase hexadecimal digits.

A reader MUST compare the digest of the bytes it downloaded with `sha256` before it keeps or runs
them. A target that is not in `files` is a machine the release has no program for.

### 4.5 Addresses

An address is an `https:` or a `file:` URI of at most 2048 bytes made only of printable ASCII
(`%x21-7E`): it begins with `https://` and has something after it, or with `file:///`. Any other
scheme, a relative address and an address with a space in it are errors. `http:` is one of them.

Addresses are absolute and are part of what is signed. A relative address would be resolved against
where the reader found the file, and a publisher such as GitHub answers the stable address
`releases/latest/download/<file>` by redirecting twice, to a place whose path says nothing of the
release. `file:` addresses are for an author's own trials and for tests: a reader MAY refuse them.

### 4.6 Previous releases

Each entry of `previous` is an object with the members `version` (4.2), `protocol` and `views`
(4.3) of an earlier release, and `manifest`: the address (4.5) of that release's `app.json`.

A writer lists, for each requirement that an earlier release had and this one does not have, the
newest earlier release with it, in order of version number, the lowest first. A release whose
requirement is the same as an earlier one's makes the earlier one redundant: it is not listed.
Whoever writes the list for a new release builds it from the `app.json` of the release published
before: that file's `previous` and that file itself, without what the new release makes redundant.
When the list would be longer than 32 entries, the entries of the lowest version numbers are the
ones dropped. A reader MUST NOT rely on the order.

## 5. The signature

### 5.1 Key and signature text

The author's key is an Ed25519 key (RFC 8032). In `key` and in `app.json.sig` a key or a signature is
written as text: `ed25519:` and the standard base64 of its bytes (RFC 4648 Section 4, with padding),
32 bytes for a public key and 64 for a signature. The text is canonical: a reader rejects one that
base64 would not write for the bytes it decodes to, one with a space in it and one of the wrong
length.

`app.json.sig` is one line: the signature text, which may be followed by one line feed.

### 5.2 What is signed and how it is checked

The signature is over the bytes of `app.json`, as published. A reader checks the bytes it received,
not a re-serialisation of what it parsed from them, with the key that `key` names, and MUST reject
the release if the signature does not hold. A reader SHOULD use the strict form of Ed25519
verification (the cofactor and malleability rules of RFC 8032 Section 5.1.7, as in `verify_strict`
of the `ed25519-dalek` library), so that a signature has one form.

A reader checks the shape of the file (Section 4) before the signature, because the key it needs
is in the file; it does not act on what it has read until the signature has held.

## 6. Compatibility

A release **opens** in a reader that speaks the host side of `pencere.md` when:

- the host speaks at least one of the release's `protocol` versions (Section 5.4: a version of the
  protocol is chosen from those both ends speak), and
- the host would list at least one kind in its `welcome` if the release's `views` were the `views`
  of a `hello` (Section 5.2): the kind is in the host's table, and shares at least one version with
  the host. A kind the host does not open, a name that is not in the table of Section 5.5 and a
  kind with no version in common do not count.

The rule is the handshake's own, not a copy of it: a release a host would find nothing to open from
is a release it does not open, and the other way round. Nothing else decides compatibility: not the
name of the file, not the date of the release, not a number of the host's own.

## 7. Choosing a release

A reader that has verified an `app.json` for a host chooses among that release and the entries of
its `previous` those that open (Section 6), and takes the one with the **greatest version number**
(4.2). It does not look at the date a release was published, nor at the order of the list: a
`0.4.4` released after `0.5.1` is the latest release and `0.5.1` is in its `previous`, and a host
that opens both is rightly given `0.5.1`.

If the chosen entry is one of `previous`, the reader fetches the `app.json` at its `manifest`
address, checks it as above, and finds its `files`. It MUST check that the file it fetched is the
release the entry named (the same `name` and `version`) and SHOULD require that it names the same
`key`; whether it accepts another key is a decision of Section 8.

If nothing opens, the reader has nothing to give the host, and it knows what would serve: the
`version`, `protocol` and `views` of the release it holds are the requirement of the latest
release. A terminal can tell its person that the app needs a newer terminal.

## 8. Trust

This document does not say which keys a reader trusts, and a signature that holds is not a reason
to run a program: it says only that whoever holds the key released the file, and the key is in the
file. A reader decides which keys it believes. A terminal may carry keys of its own, ask its person
once, and remember the answer for the app's later releases, which then must be signed with the same
key; another terminal may keep a different list. No member of `app.json` carries trust, and a
reader MUST NOT treat the name of an app as a claim about who made it.

What a reader puts on a machine runs there with the authority of the person, and sees the machine's
files. A reader that installs apps SHOULD say so to the person who asks for one.

## 9. Security considerations

- `app.json` and `app.json.sig` come from the network. Section 3 bounds what is read; Sections 4
  and 5 say what is checked; a reader that does either in another order or does less is not a
  reader of this document.
- A person's key is theirs: it is stored by whoever signs, and a release signed by a key that is
  lost cannot be followed by a release signed by another one without the reader's help. This
  document defines no way to change a key.
- A digest and a signature guard what was published. Neither says the program is harmless.
- `previous` lets a reader fall back to a release older than the latest. It is the author's list,
  signed with the rest of the file, so an attacker who cannot sign cannot add an entry; an old
  release stays what it was when it was signed, and a reader that wants to refuse a release that is
  known to be bad does so by its own list.

## 10. Examples

The release of the app `demopen`, version `0.5.1`, which needs the `lua` view at version `0.3` and
lists the release `0.4.3`, which needs `0.2`. The key is a key made for the fixtures, whose secret
half is published with them so that anyone can reproduce the files (`fixtures/README.md`); it is
for tests and nothing else.

```file fixtures/apps/valid/app.json
{
  "name": "demopen",
  "version": "0.5.1",
  "protocol": [1],
  "views": [{"kind": "lua", "versions": ["0.3"]}],
  "key": "ed25519:A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=",
  "files": {
    "aarch64-apple-darwin": {
      "url": "https://github.com/example/demopen/releases/download/v0.5.1/demopen-0.5.1-aarch64-apple-darwin",
      "sha256": "f3ebb5789dd14196c3b2dd253381a56edba01fdd3d0662f2b7048fa2296c7cdf"
    },
    "x86_64-unknown-linux-musl": {
      "url": "https://github.com/example/demopen/releases/download/v0.5.1/demopen-0.5.1-x86_64-unknown-linux-musl",
      "sha256": "f14005f9410179d8e426c41285e4cbff4203b2d74af13491ec2dad13264e474c"
    }
  },
  "previous": [
    {
      "version": "0.4.3",
      "protocol": [1],
      "views": [{"kind": "lua", "versions": ["0.2"]}],
      "manifest": "https://github.com/example/demopen/releases/download/v0.4.3/app.json"
    }
  ]
}
```

Its signature, one line:

```file fixtures/apps/valid/app.json.sig
ed25519:7SvvkWdzpnlQ+oC93Tfy/BAgojUn3VH2TaVVV/AXWUp4id84zylf3iCOnuPfaxvJDL1EHdl3VtatTq9iifecBg==
```

And the key as the file names it:

```file fixtures/apps/key.pub
ed25519:A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=
```
