# pencere-page

The SDK a page module of [pencere](https://github.com/bateri/pencere)'s native views is written
with. A page is a type that says what it looks like for a viewport and what it does when the
person does something; `export_page!` makes the WebAssembly component of it, which a terminal runs
in its sandbox (`pencere-sandbox`) and draws in its own font and theme.

```rust
use pencere_page::{Event, Frame, Layout, Page, Reply, Size, div, export_page, text};

struct Hello;

impl Page for Hello {
    fn new() -> Self { Hello }
    fn render(&mut self, _size: Size, _now: f64, _reduce_motion: bool) -> Frame {
        Frame::still(div().p(16.0).child(text("hello")))
    }
    fn settle(&mut self, _layout: &Layout) -> bool { false }
    fn handle(&mut self, _event: Event, _layout: &Layout) -> Reply { Reply::none() }
}

export_page!(Hello);
```

What a page looks like is written in the style language of the page engine (`pencere-view`,
re-exported here): `div().row().gap(8.0).child(...)`, in points, with colours named for the
theme. The component imports nothing but the page's interface (`pencere:view`): no file, no
network, no clock. A page that a program in the pane opened can talk to that program with `say`
and `Page::message`, in the six messages of the wire (`pencere-wire`, re-exported as `messages`).

## Building a page

A page is a library of crate type `cdylib`, built for `wasm32-unknown-unknown`; the module that
comes out is made a component (`wasm-tools component new`, or the same call as a library,
`wit-component`'s `ComponentEncoder`). The crate reads the interface from its own copy,
`wit/view.wit`, and the component carries it.

The crate pins `wit-bindgen` to the release the sandbox's runtime was written against.

## License

Apache-2.0 — see [LICENSE](LICENSE).
