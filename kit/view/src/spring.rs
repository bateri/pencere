//! A damped spring, in the closed form: where it is, and how fast it goes, a time after where it
//! was. One place for it: a caret's glide (critically damped, a terminal's `cursor_motion`) and the
//! springs of a page's motion ([`crate::Ease::Spring`], which may bounce) both go by it.

/// Where a spring is after `dt` seconds, and how fast it goes then, as its distance from its rest
/// (`error`: it is at rest where that is 0) and its speed (in the unit of the error, a second).
/// `omega` is how stiff it is (radians a second: it comes to rest in about `10 / omega` seconds)
/// and `zeta` how damped: 1 comes to rest with no overshoot, less overshoots and swings back (0.1
/// bounces for a long time), and more than 1 is 1. The closed form is right however long the step.
#[must_use]
pub fn spring_step(error: f32, speed: f32, omega: f32, zeta: f32, dt: f32) -> (f32, f32) {
    let zeta = zeta.clamp(0.0, 1.0);
    if zeta >= 1.0 - 1.0e-4 {
        // Critical: the sum of the two roots of the one it has.
        let c = speed + omega * error;
        let decay = (-omega * dt).exp();
        let after = (error + c * dt) * decay;
        let speed = (c - omega * (error + c * dt)) * decay;
        return (after, speed);
    }
    let damped = omega * (1.0 - zeta * zeta).sqrt();
    let decay = (-zeta * omega * dt).exp();
    let (sin, cos) = (damped * dt).sin_cos();
    let b = (speed + zeta * omega * error) / damped;
    let after = decay * (error * cos + b * sin);
    let speed = decay
        * ((b * damped - zeta * omega * error) * cos - (zeta * omega * b + error * damped) * sin);
    (after, speed)
}

#[cfg(test)]
mod tests {
    use super::spring_step;

    /// The spring stepped in `n` small steps, to compare with one long one.
    fn stepped(error: f32, speed: f32, omega: f32, zeta: f32, seconds: f32, n: u16) -> (f32, f32) {
        let dt = seconds / f32::from(n);
        (0..n).fold((error, speed), |(error, speed), _| {
            spring_step(error, speed, omega, zeta, dt)
        })
    }

    #[test]
    fn a_critical_spring_is_the_one_a_caret_glides_by() {
        // From rest at 1, `(1 + ωt) e^(-ωt)`.
        let omega = 20.0_f32;
        for t in [0.01_f32, 0.05, 0.2, 0.5] {
            let (error, _) = spring_step(1.0, 0.0, omega, 1.0, t);
            let expected = (1.0 + omega * t) * (-omega * t).exp();
            assert!((error - expected).abs() < 1.0e-6, "{t}: {error} {expected}");
        }
    }

    #[test]
    fn a_spring_is_the_same_in_one_long_step_and_in_many_short_ones() {
        for zeta in [1.0_f32, 0.7, 0.4, 0.1] {
            let (one, one_speed) = spring_step(1.0, 3.0, 30.0, zeta, 0.3);
            let (many, many_speed) = stepped(1.0, 3.0, 30.0, zeta, 0.3, 300);
            assert!((one - many).abs() < 2.0e-3, "{zeta}: {one} {many}");
            assert!(
                (one_speed - many_speed).abs() < 2.0e-2,
                "{zeta}: {one_speed} {many_speed}"
            );
        }
    }

    #[test]
    fn a_spring_that_is_not_critical_goes_past_its_rest_and_comes_back() {
        let omega = 30.0_f32;
        let mut past = false;
        let mut error = 1.0_f32;
        let mut speed = 0.0_f32;
        for _ in 0..200 {
            (error, speed) = spring_step(error, speed, omega, 0.4, 0.005);
            past |= error < -0.05;
        }
        assert!(past, "it went to the other side");
        assert!(
            error.abs() < 1.0e-3 && speed.abs() < 0.05,
            "and settled: {error} {speed}"
        );
        // A critical one never does.
        let (mut error, mut speed) = (1.0_f32, 0.0_f32);
        for _ in 0..200 {
            (error, speed) = spring_step(error, speed, omega, 1.0, 0.005);
            assert!(error >= -1.0e-6, "{error}");
        }
    }

    #[test]
    fn a_spring_given_more_than_critical_is_critical() {
        assert_eq!(
            spring_step(1.0, 0.0, 20.0, 3.0, 0.1),
            spring_step(1.0, 0.0, 20.0, 1.0, 0.1)
        );
    }
}
