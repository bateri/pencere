//! A program in this process, with the program library's own handlers (`pencere::Conversation`):
//! the program a page under the headless host speaks to when a test has its `Options` at hand.

use std::collections::BTreeSet;
use std::process::{Command, ExitStatus};
use std::time::{Duration, Instant};

use pencere::{Conversation, Lent, Options, Said};
use pencere_wire::messages::Message;

use super::Peer;

/// The most a host takes of what the program said at once, bytes: what a terminal reads at once. A
/// handler that says more waits for the next taking, as it waits for a terminal, and one that never
/// stops cannot hold the host in a single taking.
const READ: usize = 64 * 1024;

/// A program's handlers served in this process — its calls, lanes, topics and control, from the
/// same [`Options`] it gives `pencere::serve`, served by the same code ([`Conversation`]) — as the
/// program a page under the headless host speaks to. What the handlers say comes on their own
/// threads and in their own time: a settling of a page at rest waits for the answers they owe
/// ([`Peer::busy`]), and `Headless::wait_for` for what they say of themselves.
///
/// What the program does to its view ends the page's, as a terminal ends it, and the test opens the
/// page anew with the same program (`Headless::into_peer`) where a terminal would show it again:
///
/// - it **closes** its view: the page's is over (`Error::Closed`), and the program shows none
///   again — a page opened with it after is over at once;
/// - it **shows another** in its place: the page's view is over, and the program speaks to the page
///   opened next with it, as to the view a terminal opens in its place (which of the kinds of view
///   it brings is shown is a terminal's choice: the test opens the page it means to);
/// - it **lends the terminal** to a command, which no terminal runs here: the page's view is over
///   until the test says how the command ended ([`InProcess::lent`], [`InProcess::give_back`]), as a
///   person's editor or shell ends, and the program speaks to the page opened next with it, as to
///   the view a terminal shows again.
///
/// A page is opened with the program only for a view the program shows: the first, or one it shows
/// anew. A page opened on a view an earlier page had (it ended, or the test put it away) is over at
/// once — the program still holds what the earlier one asked.
pub struct InProcess {
    conversation: Conversation,
    /// What the program said to the page that the host has not taken.
    said: Vec<String>,
    /// The page's calls the program has not answered yet, by their numbers.
    owed: BTreeSet<u64>,
    /// The terminal the program lent to a command, until the test says how it ended.
    lent: Option<Lent>,
    /// Why the page's view is over, once it is; until a page is opened anew.
    over: Option<String>,
    /// The program closed its view: it shows none again.
    ended: bool,
    /// The view the program shows now has had no page yet.
    fresh: bool,
}

impl InProcess {
    /// The handlers of `options`, serving a view that is up.
    pub fn new(options: Options) -> Self {
        Self {
            conversation: Conversation::new(options),
            said: Vec::new(),
            owed: BTreeSet::new(),
            lent: None,
            over: None,
            ended: false,
            fresh: true,
        }
    }

    /// The command the program lent the terminal to, until the test says how it ended
    /// ([`InProcess::give_back`]).
    pub fn lent(&self) -> Option<&Command> {
        self.lent.as_ref().map(Lent::command)
    }

    /// The command the program lent the terminal to ended with `status`: the program is told, and
    /// shows its view again — to the page opened next with it. Whether a command had the terminal.
    pub fn give_back(&mut self, status: ExitStatus) -> bool {
        let Some(lent) = self.lent.take() else {
            return false;
        };
        lent.returned(status);
        // The hand-off is over at the conversation's next word: a reopening the program asked for
        // meanwhile is said then, and is the view shown again, not another.
        self.take(None);
        true
    }

    /// Takes what the program said into what the page is told, at most [`READ`] bytes of it, waiting
    /// `within` for the first of it.
    fn take(&mut self, within: Option<Duration>) {
        let mut next = match within {
            Some(within) => self.conversation.receive(within),
            None => self.conversation.try_receive(),
        };
        let mut taken = 0;
        while let Some(said) = next {
            match said {
                Said::Message(text) => {
                    if let Ok(Message::Response(response)) = Message::parse(&text) {
                        self.owed.remove(&response.id);
                    }
                    taken += text.len();
                    self.said.push(text);
                }
                Said::Closed => {
                    self.ended = true;
                    self.end("the program closed its view".to_owned());
                }
                Said::Reopened(_) => {
                    self.end("the program showed its view anew, in place of this one".to_owned());
                }
                Said::HandedOff(lent) => {
                    let program = lent.command().get_program().to_string_lossy().into_owned();
                    self.lent = Some(lent);
                    self.end(format!("the program lent the terminal to `{program}`"));
                }
                // What a later version of the library says, which this one does not know.
                _ => {}
            }
            if taken >= READ {
                return;
            }
            next = self.conversation.try_receive();
        }
    }

    /// The page's view is over: what it asked is (the conversation dropped it). Any view the program
    /// shows from here on is a new one.
    fn end(&mut self, why: String) {
        self.owed.clear();
        self.fresh = true;
        self.over.get_or_insert(why);
    }
}

impl Peer for InProcess {
    fn opened(&mut self) {
        // What the program said to a page that is gone is not this one's.
        self.said.clear();
        self.owed.clear();
        if self.ended || self.lent.is_some() {
            // Over still, as it was.
            return;
        }
        if self.fresh {
            self.fresh = false;
            self.over = None;
        } else {
            self.over = Some(
                "the program's view is an earlier page's: a page is opened anew for a view the \
                 program shows anew"
                    .to_owned(),
            );
        }
    }

    fn heard(&mut self, message: &str) {
        // A page whose view is over speaks to nobody: not to the view the program shows next.
        if self.over.is_some() {
            return;
        }
        let taken = self.conversation.send(message);
        match Message::parse(message) {
            Ok(Message::Call(call)) if taken => {
                self.owed.insert(call.id);
            }
            Ok(Message::Cancel(cancel)) => {
                self.owed.remove(&cancel.id);
            }
            _ => {}
        }
    }

    fn said(&mut self) -> Vec<String> {
        self.take(None);
        std::mem::take(&mut self.said)
    }

    fn busy(&mut self) -> bool {
        !self.owed.is_empty() && self.over.is_none()
    }

    fn wait(&mut self, within: Duration) {
        let deadline = Instant::now() + within;
        while self.said.is_empty() && self.over.is_none() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            self.take(Some(left));
        }
    }

    fn closed(&mut self) -> Option<String> {
        self.over.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use pencere::{Options, ReopenResult, ViewControl};
    use serde_json::{Value, json};

    use super::{InProcess, READ};
    use crate::headless::Peer;

    const ADD: &str = r#"{"call":{"id":2,"method":"add","params":{"a":2,"b":3}}}"#;

    /// Slowly, as a handler may.
    fn adding() -> Options {
        Options::new().call("add", |params, _call| {
            thread::sleep(Duration::from_millis(100));
            let number = |name: &str| params.get(name).and_then(Value::as_i64).unwrap_or(0);
            Ok(json!(number("a") + number("b")))
        })
    }

    /// A program that never stops pushing is taken a terminal's read at a time: its handler waits
    /// for the next taking, and no taking waits for it to stop.
    #[test]
    fn a_program_that_never_stops_pushing_is_taken_a_read_at_a_time() {
        let mut program = InProcess::new(Options::new().topic("count", |_params, subscription| {
            let feed = subscription.clone();
            thread::spawn(move || {
                let mut n = 0u64;
                while feed.push(json!({ "n": n })) {
                    n += 1;
                }
            });
            Ok(())
        }));
        program.opened();
        program.heard(r#"{"subscribe":{"id":1,"topic":"count"}}"#);
        let (mut takings, began) = (0, Instant::now());
        while takings < 3 {
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "nothing was pushed"
            );
            let said = program.said();
            let bytes: usize = said.iter().map(String::len).sum();
            assert!(bytes < READ + 64, "{bytes} bytes taken at once");
            takings += usize::from(!said.is_empty());
        }
    }

    /// A page speaks to the view it was opened for and no other: the words of a page whose view the
    /// program showed anew reach nobody, the page opened next is the new view's, and a page opened on
    /// a view an earlier page had is over at once.
    #[test]
    fn a_page_speaks_to_the_view_it_was_opened_for_and_no_other() {
        let control = ViewControl::new();
        let mut program = InProcess::new(adding().control(control.clone()));
        program.opened();
        let reopened = thread::spawn(move || control.reopen(None));
        program.wait(Duration::from_secs(5));
        assert!(program.closed().is_some_and(|why| why.contains("anew")));
        assert!(matches!(reopened.join(), Ok(Ok(ReopenResult::Reopened))));
        // Had it reached the new view, the new page's call of the same number would be its double,
        // and the answer the old one's.
        program.heard(r#"{"call":{"id":2,"method":"add","params":{"a":1,"b":1}}}"#);
        program.opened();
        assert_eq!(program.closed(), None);
        program.heard(ADD);
        assert!(program.busy());
        program.wait(Duration::from_secs(5));
        assert_eq!(program.said(), [r#"{"response":{"id":2,"result":5}}"#]);
        program.opened();
        assert!(
            program
                .closed()
                .is_some_and(|why| why.contains("an earlier page's"))
        );
    }
}
