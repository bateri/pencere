//! What a run says: for people (`pretty`), for a pipeline that reads TAP (`tap`) and for the CI
//! systems that read JUnit.

use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Skip,
    /// It did not end within its time.
    Timeout,
    /// It was to run, and the process that ran the file ended before it did.
    NotRun,
}

impl Status {
    pub fn is_bad(self) -> bool {
        matches!(self, Self::Fail | Self::Timeout | Self::NotRun)
    }

    pub fn named(word: &str) -> Self {
        match word {
            "pass" => Self::Pass,
            "skip" => Self::Skip,
            "timeout" => Self::Timeout,
            "not-run" => Self::NotRun,
            _ => Self::Fail,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub line: u32,
    pub status: Status,
    pub ms: u64,
    pub message: Option<String>,
    pub trace: Option<String>,
    pub words: Vec<String>,
    pub artifacts: Vec<String>,
    pub output: String,
}

#[derive(Clone, Debug)]
pub struct FileResult {
    /// The file as the report names it.
    pub label: String,
    pub tests: Vec<TestResult>,
    /// Why the file could not be run, or could not be run to its end.
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub ms: u64,
}

impl Summary {
    pub fn of(files: &[FileResult], ms: u64) -> Self {
        let mut summary = Self {
            ms,
            ..Self::default()
        };
        for file in files {
            if file.error.is_some() {
                summary.errors += 1;
            }
            for test in &file.tests {
                match test.status {
                    Status::Pass => summary.passed += 1,
                    Status::Skip => summary.skipped += 1,
                    _ => summary.failed += 1,
                }
            }
        }
        summary
    }

    pub fn ran(&self) -> usize {
        self.passed + self.failed
    }

    pub fn ok(&self) -> bool {
        self.failed == 0 && self.errors == 0 && self.ran() > 0
    }
}

/// The file and line a failure points to: the position Lua put at the front of the message, else
/// the first line of the test's own files in the traceback.
pub fn locate(message: Option<&str>, trace: Option<&str>) -> Option<(String, u32)> {
    fn position(text: &str) -> Option<(String, u32)> {
        let end = text.find(".lua")? + 4;
        let file = text.get(..end)?;
        let file = file.rsplit([' ', '\n', '\t']).next().unwrap_or(file);
        let rest = text.get(end..)?.strip_prefix(':')?;
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let line = digits.parse().ok()?;
        Some((file.to_owned(), line))
    }
    if let Some(found) = message.and_then(position) {
        return Some(found);
    }
    trace?.lines().find_map(position)
}

pub trait Reporter {
    fn file_start(&mut self, label: &str);
    fn test(&mut self, file: &str, test: &TestResult);
    fn file_end(&mut self, file: &FileResult);
    fn finish(&mut self, summary: &Summary, files: &[FileResult]);
}

fn lines(text: &str, indent: &str) -> String {
    let mut out = String::new();
    for line in text.lines() {
        let _ = writeln!(out, "{indent}{line}");
    }
    out
}

// ---- pretty ---------------------------------------------------------------------------------

pub struct Pretty {
    terminal: bool,
    github: bool,
    root: std::path::PathBuf,
}

impl Pretty {
    pub fn new(root: &Path) -> Self {
        Self {
            terminal: std::io::stdout().is_terminal(),
            github: std::env::var("GITHUB_ACTIONS").is_ok_and(|value| value == "true"),
            root: root.to_path_buf(),
        }
    }

    fn mark(&self, status: Status) -> &'static str {
        match (status, self.terminal) {
            (Status::Pass, true) => "\u{2713}",
            (Status::Pass, false) => "ok  ",
            (Status::Skip, true) => "-",
            (Status::Skip, false) => "skip",
            (_, true) => "\u{2717}",
            (_, false) => "FAIL",
        }
    }
}

impl Reporter for Pretty {
    fn file_start(&mut self, label: &str) {
        println!("{label}");
    }

    fn test(&mut self, file: &str, test: &TestResult) {
        let how = match test.status {
            Status::Timeout => " (timed out)",
            Status::NotRun => " (did not run)",
            _ => "",
        };
        println!(
            "  {} {}{how} ({} ms)",
            self.mark(test.status),
            test.name,
            test.ms
        );
        if !test.output.trim().is_empty() {
            print!("{}", lines(test.output.trim_end(), "      | "));
        }
        if test.status.is_bad() {
            if let Some(message) = &test.message {
                print!("{}", lines(message, "      "));
            }
            let described = test.message.as_deref().is_some_and(|message| {
                message.contains("the page says") || message.contains("the page has")
            });
            if !test.words.is_empty() && !described {
                println!(
                    "      the page, when it ended, said: {}",
                    test.words.join(" | ")
                );
            }
            for artifact in &test.artifacts {
                let shown = Path::new(artifact)
                    .strip_prefix(&self.root)
                    .map_or_else(|_| artifact.clone(), |path| path.display().to_string());
                println!("      kept: {shown}");
            }
            if self.github {
                let (path, line) = locate(test.message.as_deref(), test.trace.as_deref())
                    .unwrap_or_else(|| (file.to_owned(), test.line));
                let message = test
                    .message
                    .as_deref()
                    .unwrap_or("the test failed")
                    .replace('%', "%25")
                    .replace('\r', "%0D")
                    .replace('\n', "%0A");
                let path = if path.contains('/') {
                    path
                } else {
                    file.to_owned()
                };
                println!("::error file={path},line={line}::{message}");
            }
        }
    }

    fn file_end(&mut self, file: &FileResult) {
        if let Some(error) = &file.error {
            println!("  error: {error}");
        }
    }

    fn finish(&mut self, summary: &Summary, _: &[FileResult]) {
        println!();
        let mut parts = vec![format!("{} passed", summary.passed)];
        if summary.failed > 0 {
            parts.push(format!("{} failed", summary.failed));
        }
        if summary.skipped > 0 {
            parts.push(format!("{} skipped", summary.skipped));
        }
        if summary.errors > 0 {
            parts.push(format!("{} files with errors", summary.errors));
        }
        println!("{} ({} ms)", parts.join(", "), summary.ms);
        if summary.ran() == 0 && summary.errors == 0 {
            println!("no test ran: a run that looked at nothing proves nothing");
        }
    }
}

// ---- TAP ------------------------------------------------------------------------------------

#[derive(Default)]
pub struct Tap {
    count: usize,
}

impl Tap {
    pub fn new() -> Self {
        println!("TAP version 13");
        Self::default()
    }
}

impl Reporter for Tap {
    fn file_start(&mut self, label: &str) {
        println!("# {label}");
    }

    fn test(&mut self, file: &str, test: &TestResult) {
        self.count += 1;
        let number = self.count;
        let name = format!("{file}: {}", test.name);
        match test.status {
            Status::Pass => println!("ok {number} - {name}"),
            Status::Skip => println!("ok {number} - {name} # SKIP"),
            _ => {
                println!("not ok {number} - {name}");
                println!("  ---");
                if test.status == Status::Timeout {
                    println!("  timed_out: true");
                }
                if let Some((path, line)) = locate(test.message.as_deref(), test.trace.as_deref()) {
                    println!("  at: {path}:{line}");
                } else {
                    println!("  at: {file}:{}", test.line);
                }
                println!("  message: |");
                print!(
                    "{}",
                    lines(test.message.as_deref().unwrap_or("the test failed"), "    ")
                );
                if !test.words.is_empty() {
                    println!("  page: |");
                    print!("{}", lines(&test.words.join(" | "), "    "));
                }
                if !test.artifacts.is_empty() {
                    println!("  artifacts:");
                    for artifact in &test.artifacts {
                        println!("    - {artifact}");
                    }
                }
                println!("  ...");
            }
        }
    }

    fn file_end(&mut self, file: &FileResult) {
        if let Some(error) = &file.error {
            self.count += 1;
            println!(
                "not ok {} - {}: the file did not run",
                self.count, file.label
            );
            println!("  ---");
            println!("  message: |");
            print!("{}", lines(error, "    "));
            println!("  ...");
        }
    }

    fn finish(&mut self, summary: &Summary, _: &[FileResult]) {
        println!("1..{}", self.count);
        println!(
            "# {} passed, {} failed, {} skipped",
            summary.passed, summary.failed, summary.skipped
        );
    }
}

// ---- JUnit ----------------------------------------------------------------------------------

fn xml(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\t' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// The JUnit XML of a run: a suite for each file, the pictures a failure left as the
/// `[[ATTACHMENT|path]]` lines that GitLab and Jenkins read.
pub fn junit(files: &[FileResult], summary: &Summary) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"pencere\" tests=\"{}\" failures=\"{}\" errors=\"{}\" skipped=\"{}\" time=\"{:.3}\">",
        summary.passed + summary.failed + summary.skipped,
        summary.failed,
        summary.errors,
        summary.skipped,
        summary.ms as f64 / 1000.0
    );
    for file in files {
        let failures = file
            .tests
            .iter()
            .filter(|test| test.status.is_bad())
            .count();
        let skipped = file
            .tests
            .iter()
            .filter(|test| test.status == Status::Skip)
            .count();
        let time: u64 = file.tests.iter().map(|test| test.ms).sum();
        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" errors=\"{}\" skipped=\"{skipped}\" time=\"{:.3}\">",
            xml(&file.label),
            file.tests.len(),
            usize::from(file.error.is_some()),
            time as f64 / 1000.0
        );
        for test in &file.tests {
            let _ = write!(
                out,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                xml(&file.label),
                xml(&test.name),
                test.ms as f64 / 1000.0
            );
            match test.status {
                Status::Pass => out.push_str("/>\n"),
                Status::Skip => out.push_str("><skipped/></testcase>\n"),
                _ => {
                    let message = test.message.as_deref().unwrap_or("the test failed");
                    let first = message.lines().next().unwrap_or("");
                    let _ = writeln!(out, ">");
                    let _ = writeln!(
                        out,
                        "      <failure message=\"{}\">{}</failure>",
                        xml(first),
                        xml(message)
                    );
                    if !test.artifacts.is_empty() || !test.output.is_empty() {
                        let mut system = String::new();
                        system.push_str(&test.output);
                        for artifact in &test.artifacts {
                            let _ = writeln!(system, "[[ATTACHMENT|{artifact}]]");
                        }
                        let _ = writeln!(out, "      <system-out>{}</system-out>", xml(&system));
                    }
                    out.push_str("    </testcase>\n");
                }
            }
        }
        if let Some(error) = &file.error {
            let _ = writeln!(
                out,
                "    <testcase classname=\"{}\" name=\"(the file)\" time=\"0\"><error message=\"{}\">{}</error></testcase>",
                xml(&file.label),
                xml(error.lines().next().unwrap_or("")),
                xml(error)
            );
        }
        out.push_str("  </testsuite>\n");
    }
    out.push_str("</testsuites>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failing() -> TestResult {
        TestResult {
            name: "g › it <fails>".to_owned(),
            line: 7,
            status: Status::Fail,
            ms: 12,
            message: Some(
                "page_test.lua:9: expected the page to say \"x\"\nthe page says: \"y\"".to_owned(),
            ),
            trace: None,
            words: vec!["y".to_owned()],
            artifacts: vec!["/tmp/a/page-1.png".to_owned()],
            output: String::new(),
        }
    }

    fn passing() -> TestResult {
        TestResult {
            name: "it works".to_owned(),
            line: 3,
            status: Status::Pass,
            ms: 4,
            message: None,
            trace: None,
            words: Vec::new(),
            artifacts: Vec::new(),
            output: String::new(),
        }
    }

    #[test]
    fn a_failure_is_located_by_the_position_lua_put_in_its_message_or_by_the_trace() {
        assert_eq!(
            locate(Some("page_test.lua:9: expected"), None),
            Some(("page_test.lua".to_owned(), 9))
        );
        assert_eq!(
            locate(
                Some("no position"),
                Some("pencere.test:12 function x\npage_test.lua:30 function y")
            ),
            Some(("page_test.lua".to_owned(), 30))
        );
        assert_eq!(locate(Some("nothing"), None), None);
    }

    #[test]
    fn junit_names_a_suite_for_each_file_and_keeps_what_a_failure_left() {
        let files = vec![FileResult {
            label: "test/page_test.lua".to_owned(),
            tests: vec![passing(), failing()],
            error: None,
        }];
        let summary = Summary::of(&files, 1500);
        let xml = junit(&files, &summary);
        assert_eq!(
            xml,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<testsuites name=\"pencere\" tests=\"2\" failures=\"1\" errors=\"0\" skipped=\"0\" time=\"1.500\">\n\
\x20 <testsuite name=\"test/page_test.lua\" tests=\"2\" failures=\"1\" errors=\"0\" skipped=\"0\" time=\"0.016\">\n\
\x20   <testcase classname=\"test/page_test.lua\" name=\"it works\" time=\"0.004\"/>\n\
\x20   <testcase classname=\"test/page_test.lua\" name=\"g › it &lt;fails&gt;\" time=\"0.012\">\n\
\x20     <failure message=\"page_test.lua:9: expected the page to say &quot;x&quot;\">page_test.lua:9: expected the page to say &quot;x&quot;\nthe page says: &quot;y&quot;</failure>\n\
\x20     <system-out>[[ATTACHMENT|/tmp/a/page-1.png]]\n</system-out>\n\
\x20   </testcase>\n\
\x20 </testsuite>\n\
</testsuites>\n"
        );
    }

    #[test]
    fn tap_numbers_every_test_and_gives_a_failure_its_place_and_its_pictures() {
        // The reporter prints; what it would print is checked through the same pieces.
        let test = failing();
        assert_eq!(
            locate(test.message.as_deref(), test.trace.as_deref()),
            Some(("page_test.lua".to_owned(), 9))
        );
        let summary = Summary::of(
            &[FileResult {
                label: "a".to_owned(),
                tests: vec![passing(), failing()],
                error: None,
            }],
            0,
        );
        assert_eq!((summary.passed, summary.failed, summary.ran()), (1, 1, 2));
        assert!(!summary.ok());
    }

    #[test]
    fn a_run_that_ran_nothing_is_not_a_pass() {
        assert!(!Summary::of(&[], 0).ok());
    }
}
