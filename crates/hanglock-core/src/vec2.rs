//! A 2-D point with the handful of operations the solver needs. Deliberately not a general
//! linear-algebra type: the rope is 17 points and four vector ops, and a smaller surface here is
//! a smaller surface to get wrong in the one place that must be bit-reproducible.

/// A position in logical pixels. `y` grows downward, which is canvas convention, so gravity is a
/// positive-`y` acceleration.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn add(self, o: Self) -> Self {
        Self {
            x: self.x + o.x,
            y: self.y + o.y,
        }
    }

    #[must_use]
    pub fn sub(self, o: Self) -> Self {
        Self {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }

    #[must_use]
    pub fn scale(self, k: f64) -> Self {
        Self {
            x: self.x * k,
            y: self.y * k,
        }
    }

    #[must_use]
    pub fn len(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    #[must_use]
    pub fn dist(self, o: Self) -> f64 {
        self.sub(o).len()
    }

    /// Length clamped to `max`, keeping direction. The solver's speed rail and reach clamp are
    /// both this one operation.
    #[must_use]
    pub fn limited_to(self, max: f64) -> Self {
        let m = self.len();
        if m > max && m > f64::EPSILON {
            self.scale(max / m)
        } else {
            self
        }
    }
}
