//! Every `decode` entry of the fixtures' manifest, fed to the decoders whole,
//! cut at every byte, and one byte at a time; every `canonical` entry written
//! back from its messages.

// A test fails by panicking, and indexes what it has just built.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use pencere_host::frame::{
    BodyDecoder, DecodeError, Direction, Event, Message, Outgoing, Split, StreamDecoder, encode,
};
use serde_json::Value;

/// A message as the manifest writes it: kind, id, control, payload.
type Expected = (String, u32, Vec<(String, String)>, String);

struct Entry {
    file: String,
    direction: Direction,
    scope: String,
    canonical: bool,
    messages: Vec<Expected>,
    errors: Vec<String>,
}

fn entries() -> Vec<Entry> {
    let manifest = common::manifest();
    let list = manifest["decode"].as_array().unwrap();
    list.iter()
        .map(|entry| Entry {
            file: entry["file"].as_str().unwrap().to_owned(),
            direction: Direction::from_name(entry["direction"].as_str().unwrap()).unwrap(),
            scope: entry["scope"].as_str().unwrap().to_owned(),
            canonical: entry["canonical"].as_bool().unwrap(),
            messages: entry["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| {
                    let mut control: Vec<(String, String)> = m["control"]
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
                        .collect();
                    control.sort();
                    (
                        m["kind"].as_str().unwrap().to_owned(),
                        u32::try_from(m["id"].as_u64().unwrap()).unwrap(),
                        control,
                        m["payload"].as_str().unwrap().to_owned(),
                    )
                })
                .collect(),
            errors: entry["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_owned())
                .collect(),
        })
        .collect()
}

fn summary(events: &[Event]) -> (Vec<Expected>, Vec<String>) {
    let mut messages = Vec::new();
    let mut errors = Vec::new();
    for event in events {
        match event {
            Event::Message(m) => {
                let mut control = m.control.clone();
                control.sort();
                messages.push((m.kind.clone(), m.id, control, m.payload.clone()));
            }
            Event::Error(e) => errors.push(e.name().to_owned()),
        }
    }
    (messages, errors)
}

fn bodies(bytes: &[u8]) -> Vec<&[u8]> {
    common::bodies(bytes)
}

fn decode_stream(direction: Direction, pieces: &[&[u8]]) -> Vec<Event> {
    let mut decoder = StreamDecoder::new(direction);
    let mut out = Vec::new();
    for piece in pieces {
        decoder.feed(piece, &mut out);
    }
    out
}

#[test]
fn the_manifest_lists_what_the_directory_holds() {
    let mut listed: Vec<String> = entries().into_iter().map(|e| e.file).collect();
    let manifest = common::manifest();
    for session in manifest["session"].as_array().unwrap() {
        for step in session["steps"].as_array().unwrap() {
            for key in ["in", "out"] {
                if let Some(file) = step[key].as_str() {
                    listed.push(file.to_owned());
                }
            }
        }
    }
    let mut found = Vec::new();
    let mut stack = vec![common::root()];
    while let Some(dir) = stack.pop() {
        for item in std::fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "frame") {
                let relative = path.strip_prefix(common::root()).unwrap();
                found.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    listed.sort();
    found.sort();
    assert_eq!(listed, found);
}

#[test]
fn every_entry_decodes_as_the_manifest_says_however_the_stream_is_cut() {
    for entry in entries() {
        let bytes = common::file(&entry.file);
        let expected = (entry.messages.clone(), entry.errors.clone());
        let whole = decode_stream(entry.direction, &[&bytes]);
        assert_eq!(summary(&whole), expected, "{}", entry.file);
        for at in 0..=bytes.len() {
            let events = decode_stream(entry.direction, &[&bytes[..at], &bytes[at..]]);
            assert_eq!(events, whole, "{} cut at {at}", entry.file);
        }
        let singles: Vec<&[u8]> = bytes.chunks(1).collect();
        assert_eq!(
            decode_stream(entry.direction, &singles),
            whole,
            "{}",
            entry.file
        );
    }
}

#[test]
fn body_scope_entries_decode_from_apc_bodies_cut_anywhere() {
    let mut run = 0;
    for entry in entries().into_iter().filter(|e| e.scope == "body") {
        run += 1;
        let bytes = common::file(&entry.file);
        let expected = (entry.messages.clone(), entry.errors.clone());
        let bodies = bodies(&bytes);
        let feed = |cut: Option<(usize, usize)>, step: usize| {
            let mut decoder = BodyDecoder::new(entry.direction);
            let mut out = Vec::new();
            for (index, body) in bodies.iter().enumerate() {
                decoder.start();
                match cut {
                    Some((which, at)) if which == index => {
                        decoder.put(&body[..at], &mut out);
                        decoder.put(&body[at..], &mut out);
                    }
                    _ => {
                        for piece in body.chunks(step.max(1)) {
                            decoder.put(piece, &mut out);
                        }
                    }
                }
                decoder.end(false, &mut out);
            }
            summary(&out)
        };
        assert_eq!(feed(None, usize::MAX), expected, "{} whole", entry.file);
        assert_eq!(feed(None, 1), expected, "{} byte by byte", entry.file);
        for (index, body) in bodies.iter().enumerate() {
            for at in 0..=body.len() {
                assert_eq!(
                    feed(Some((index, at)), usize::MAX),
                    expected,
                    "{} body {index} cut at {at}",
                    entry.file
                );
            }
        }
    }
    assert!(run >= 30, "only {run} body entries");
}

fn written(direction: Direction, messages: &[Message], split: Split) -> Vec<u8> {
    let mut out = Vec::new();
    for m in messages {
        let msg = Outgoing {
            kind: &m.kind,
            id: m.id,
            re: m.re(),
            digest: m.digest(),
            file: pencere_host::frame::FileParams {
                handle: m.handle(),
                offset: m.offset(),
                end: m.end(),
                error: m.error_code(),
            },
            payload: &m.payload,
        };
        encode(direction, &msg, split, &mut out).unwrap();
    }
    out
}

fn messages_of(entry: &Entry) -> Vec<Message> {
    entry
        .messages
        .iter()
        .map(|(kind, id, control, payload)| Message {
            kind: kind.clone(),
            id: *id,
            control: control.clone(),
            payload: payload.clone(),
        })
        .collect()
}

#[test]
fn a_canonical_entry_is_what_the_writer_writes() {
    let mut count = 0;
    for entry in entries().into_iter().filter(|e| e.canonical) {
        count += 1;
        let bytes = common::file(&entry.file);
        let rewritten = written(entry.direction, &messages_of(&entry), Split::Fill);
        assert_eq!(
            String::from_utf8_lossy(&rewritten),
            String::from_utf8_lossy(&bytes),
            "{}",
            entry.file
        );
    }
    assert!(count >= 19, "only {count} canonical entries");
}

#[test]
fn the_two_chunked_examples_are_what_a_sender_that_picks_those_sizes_writes() {
    for (file, size) in [
        ("to-host/file-chunked.frame", 60),
        ("to-host/msg-chunked.frame", 59),
    ] {
        let entry = entries().into_iter().find(|e| e.file == file).unwrap();
        let bytes = common::file(file);
        let rewritten = written(entry.direction, &messages_of(&entry), Split::Fixed(size));
        assert_eq!(
            String::from_utf8_lossy(&rewritten),
            String::from_utf8_lossy(&bytes),
            "{file}"
        );
    }
}

#[test]
fn the_errors_the_fixtures_name_are_the_ones_the_crate_has() {
    let manifest = common::manifest();
    let mut names: Vec<&str> = manifest["decode"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|e| {
            e["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap())
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    for name in &names {
        assert!(
            [
                DecodeError::Malformed,
                DecodeError::TooLong,
                DecodeError::ChunkOrder,
                DecodeError::Abandoned,
                DecodeError::MessageTooLong,
            ]
            .iter()
            .any(|e| e.name() == *name),
            "{name}"
        );
    }
    assert_eq!(manifest["protocol"], Value::from(1));
}
