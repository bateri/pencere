//! What a page asks of a trackpad's motor: a vibration, as a pattern of events that any device
//! that can vibrate can be given, and the names of the patterns the kit has ready.
//!
//! A pattern says what a person is to feel, not what a motor is to do: each event has a strength
//! (`intensity`) and a sharpness (`sharpness`, from a dull thud to a crisp click), both from 0 to
//! 1, and a time in seconds. An event is a *tap* (0 seconds) or is *held* for a
//! time, its strength and its sharpness going straight from where they begin to where they end.
//! That is the family of what the platforms' own haptic libraries take, so that a pattern is
//! carried to another device as it is.
//!
//! A strength is a share of the strongest the device gives: 1 is all of it and 0 nothing. It is
//! the one scale for every pattern, the kit's ready ones and a page's own, so that a number means
//! the same wherever it is written. Which frequency a sharpness comes to is the host's to know: it
//! maps what it is given to what it can feel.
//!
//! The crate holds the patterns and the floors they are held to ([`MOST_EVENTS`], [`MOST_SECONDS`])
//! and plays nothing: no platform's library is linked.

/// The most events one pattern has.
pub const MOST_EVENTS: usize = 256;

/// The most seconds one pattern lasts: its last event ends by then.
pub const MOST_SECONDS: f32 = 60.0;

/// One thing the person is to feel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// Seconds from the start of the pattern.
    pub at: f32,
    /// Seconds the event is held: 0 for a tap.
    pub seconds: f32,
    /// How strong, from 0 to 1, where the event begins.
    pub intensity: f32,
    /// How strong where it ends (a tap's is its beginning's).
    pub intensity_end: f32,
    /// How sharp, from 0 (dull) to 1 (crisp), where the event begins.
    pub sharpness: f32,
    /// How sharp where it ends (a tap's is its beginning's).
    pub sharpness_end: f32,
}

impl Event {
    /// A tap at `at` seconds.
    #[must_use]
    pub fn tap(at: f32, intensity: f32, sharpness: f32) -> Self {
        Self {
            at,
            seconds: 0.0,
            intensity,
            intensity_end: intensity,
            sharpness,
            sharpness_end: sharpness,
        }
    }

    /// An event held for `seconds` seconds from `at`, its strength going from `intensity.0` to
    /// `intensity.1` and its sharpness from `sharpness.0` to `sharpness.1`.
    #[must_use]
    pub fn held(at: f32, seconds: f32, intensity: (f32, f32), sharpness: (f32, f32)) -> Self {
        Self {
            at,
            seconds,
            intensity: intensity.0,
            intensity_end: intensity.1,
            sharpness: sharpness.0,
            sharpness_end: sharpness.1,
        }
    }

    fn numbers(&self) -> [f32; 6] {
        [
            self.at,
            self.seconds,
            self.intensity,
            self.intensity_end,
            self.sharpness,
            self.sharpness_end,
        ]
    }
}

/// Why events are not a pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A number in an event is not one (not finite).
    NotANumber,
    /// More than [`MOST_EVENTS`] events.
    TooMany,
    /// An event ends past [`MOST_SECONDS`].
    TooLong,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, to: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotANumber => to.write_str("a number of an event is not a number"),
            Self::TooMany => write!(to, "a pattern has at most {MOST_EVENTS} events"),
            Self::TooLong => write!(to, "a pattern lasts at most {MOST_SECONDS} seconds"),
        }
    }
}

/// Events in the order of their times, each held to its range, inside the floors: what a host is
/// given, and need not check again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pattern {
    events: Vec<Event>,
}

impl Pattern {
    /// The pattern of `events`: the times from 0, the strengths and sharpnesses from 0 to 1 (a number
    /// past them is the nearest), the events in the order of their times (those at one time as they
    /// were given). A number that is not one, more than [`MOST_EVENTS`] events, or an event that
    /// ends past [`MOST_SECONDS`] is not a pattern. No events is a pattern: it plays nothing.
    ///
    /// # Errors
    /// See [`Refusal`].
    pub fn new(events: impl IntoIterator<Item = Event>) -> Result<Self, Refusal> {
        let mut held = Vec::new();
        for event in events {
            if held.len() == MOST_EVENTS {
                return Err(Refusal::TooMany);
            }
            if event.numbers().iter().any(|number| !number.is_finite()) {
                return Err(Refusal::NotANumber);
            }
            let event = Event {
                at: event.at.max(0.0),
                seconds: event.seconds.max(0.0),
                intensity: event.intensity.clamp(0.0, 1.0),
                intensity_end: event.intensity_end.clamp(0.0, 1.0),
                sharpness: event.sharpness.clamp(0.0, 1.0),
                sharpness_end: event.sharpness_end.clamp(0.0, 1.0),
            };
            if event.at + event.seconds > MOST_SECONDS {
                return Err(Refusal::TooLong);
            }
            held.push(event);
        }
        held.sort_by(|one, other| one.at.total_cmp(&other.at));
        Ok(Self { events: held })
    }

    /// The events, in the order of their times.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Whether there is nothing in it to feel.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Seconds until the last event ends.
    #[must_use]
    pub fn seconds(&self) -> f32 {
        self.events
            .iter()
            .map(|event| event.at + event.seconds)
            .fold(0.0, f32::max)
    }

    /// The strongest this pattern gets: the greatest strength any event has, where it begins or
    /// where it ends. 0 where there are no events or all of them are silent.
    #[must_use]
    pub fn peak(&self) -> f32 {
        self.events
            .iter()
            .map(|event| event.intensity.max(event.intensity_end))
            .fold(0.0, f32::max)
    }

    /// The same pattern with its strongest moment at `peak` (from 0 to 1: a number past them is the
    /// nearest, and one that is not a number leaves the pattern as it is). Every other strength keeps
    /// its share of the strongest, so the shape stays: how sharp it is, and when, are not touched. A
    /// pattern that is silent has no shape to keep, and stays as it is.
    #[must_use]
    pub fn with_peak(&self, peak: f32) -> Self {
        let most = self.peak();
        if peak.is_nan() || most <= 0.0 {
            return self.clone();
        }
        let peak = peak.clamp(0.0, 1.0);
        // Each strength is divided by the strongest and then given the new one: the strongest comes
        // to `peak` itself and none to more, whatever the rounding of the others.
        let at = |strength: f32| strength / most * peak;
        Self {
            events: self
                .events
                .iter()
                .map(|event| Event {
                    intensity: at(event.intensity),
                    intensity_end: at(event.intensity_end),
                    ..*event
                })
                .collect(),
        }
    }
}

/// What a page asks of the vibration of its view.
#[derive(Clone, Debug, PartialEq)]
pub enum Haptic {
    /// Play this, in place of what the view is playing.
    Play(Pattern),
    /// End what the view is playing.
    Stop,
}

/// The names of the patterns the kit has ready, in the order a page is told of them.
pub const NAMES: [&str; 13] = [
    "tick",
    "tap",
    "thud",
    "snap",
    "bump",
    "double",
    "success",
    "warning",
    "error",
    "rise",
    "fall",
    "rumble",
    "heartbeat",
];

/// The ready pattern called `name` (one of [`NAMES`]), or none. The names are the API; what each one
/// is made of may change, as long as it feels like what it is called. Their strengths are shares of
/// the strongest the device gives, and the strongest moment of any of them is 0.3: a page that wants
/// one stronger or lighter says how strong its strongest moment is ([`Pattern::with_peak`]).
#[must_use]
pub fn preset(name: &str) -> Option<Pattern> {
    let events = match name {
        // The lightest, crisp: a thing under the finger moved by a step (a pick in a list, a tab).
        "tick" => vec![Event::tap(0.0, 0.18, 0.9)],
        // A press that landed: a little fuller than a tick, a little less sharp.
        "tap" => vec![Event::tap(0.0, 0.225, 0.6)],
        // Something heavy set down: a strong dull knock that dies away.
        "thud" => vec![
            Event::tap(0.0, 0.3, 0.1),
            Event::held(0.0, 0.07, (0.21, 0.0), (0.1, 0.1)),
        ],
        // Something clicked into place: the strongest, the crispest.
        "snap" => vec![Event::tap(0.0, 0.3, 1.0)],
        // Running into an edge: a soft dull knock.
        "bump" => vec![Event::tap(0.0, 0.21, 0.25)],
        // Two taps close together.
        "double" => vec![Event::tap(0.0, 0.21, 0.6), Event::tap(0.12, 0.21, 0.6)],
        // It worked: a light tap, then a stronger, sharper one.
        "success" => vec![Event::tap(0.0, 0.18, 0.5), Event::tap(0.1, 0.3, 0.9)],
        // Look at this: two strong taps, the second weaker, with a pause between.
        "warning" => vec![Event::tap(0.0, 0.27, 0.7), Event::tap(0.18, 0.18, 0.5)],
        // It failed: three heavy, dull knocks.
        "error" => vec![
            Event::tap(0.0, 0.3, 0.3),
            Event::tap(0.11, 0.3, 0.3),
            Event::tap(0.22, 0.3, 0.3),
        ],
        // Building: a held buzz that grows stronger and sharper.
        "rise" => vec![Event::held(0.0, 0.4, (0.09, 0.3), (0.2, 0.8))],
        // Fading: a held buzz that dies away and dulls.
        "fall" => vec![Event::held(0.0, 0.4, (0.3, 0.09), (0.8, 0.2))],
        // A steady dull growl.
        "rumble" => vec![Event::held(0.0, 0.8, (0.21, 0.21), (0.1, 0.1))],
        // Two beats and a rest, twice.
        "heartbeat" => vec![
            Event::tap(0.0, 0.27, 0.2),
            Event::tap(0.18, 0.18, 0.2),
            Event::tap(0.9, 0.27, 0.2),
            Event::tap(1.08, 0.18, 0.2),
        ],
        _ => return None,
    };
    Some(Pattern { events })
}
