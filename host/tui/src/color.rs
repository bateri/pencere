//! Colours as the grid holds them.

/// A colour of the grid: sRGB, eight bits a channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
        }
    }

    /// Each channel `0.0..=1.0`, rounded to the nearest of the 256 (a value outside it is the end
    /// it is past).
    pub fn from_unit(channels: [f32; 3]) -> Self {
        let [r, g, b] = channels.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
        Self { r, g, b }
    }

    /// This colour `amount` of the way toward `toward`, channel by channel in sRGB, each rounded to
    /// the nearest value (halves away from zero): the kit's rule for a mix (`kit/SPEC.md`, Section 3).
    #[must_use]
    pub fn mix(self, toward: Self, amount: f32) -> Self {
        let t = if amount.is_finite() {
            amount.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let channel = |a: u8, b: u8| {
            let (a, b) = (f32::from(a), f32::from(b));
            (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
        };
        Self {
            r: channel(self.r, toward.r),
            g: channel(self.g, toward.g),
            b: channel(self.b, toward.b),
        }
    }

    /// The relative luminance, `0.0` (black) to `1.0` (white): how a theme is told dark from light.
    pub fn luminance(self) -> f32 {
        let linear = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }

    /// The contrast of two colours, `1.0` (none) to `21.0`.
    pub fn contrast(self, other: Self) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
}
