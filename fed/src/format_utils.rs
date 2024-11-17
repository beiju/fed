use std::fmt::{Display, Formatter, Write};

// Newtype with Display implementation that prints the string using grammatically correct possessive
pub struct Possessive<'a>(pub &'a str);

impl Display for Possessive<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(l) = self.0.chars().last() && l == 's' {
            write!(f, "{}'", self.0)
        } else {
            write!(f, "{}'s", self.0)
        }
    }
}

pub struct RunDisplay(pub f64);

impl Display for RunDisplay {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0.fract() == 0. {
            write!(f, "{}", self.0)
        } else {
            write!(f, "{:.1}", self.0)
        }
    }
}

// Newtype that formats runs and unruns
// TODO Should this use RunDisplay or replace it?
pub struct Runs(pub f64);

impl Display for Runs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == -1.0 {
            write!(f, "1 Unrun")
        } else if self.0 == 1.0 {
            write!(f, "1 Run")
        } else if self.0 < 0.0 {
            write!(f, "{} Unruns", -self.0)
        } else {
            write!(f, "{} Runs", self.0)
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct NewlineDelimiter {
    is_first_line: bool,
}

impl NewlineDelimiter {
    pub fn new() -> Self {
        Self {
            is_first_line: true,
        }
    }

    pub fn print(&mut self, w: &mut impl Write) -> std::fmt::Result {
        if self.is_first_line {
            self.is_first_line = false;
            Ok(())
        } else {
            write!(w, "\n")
        }
    }
}