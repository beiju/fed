use std::fmt::{Display, Formatter, Write};

// Newtype with Display implementation that prints the string using grammatically correct possessive
pub struct Possessive<'a>(pub &'a str);

impl Display for Possessive<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // TODO Let chain
        if let Some(l) = self.0.chars().last() {
            if l == 's' {
                write!(f, "{}'", self.0)
            } else {
                write!(f, "{}'s", self.0)
            }
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

impl Runs {
    pub fn unruns_always_plural(self) -> RunsWithUnrunsAlwaysPlural {
        RunsWithUnrunsAlwaysPlural(self.0)
    }

    pub fn singular_if(self, always_singular: bool) -> RunsWithSingularOverride {
        RunsWithSingularOverride {
            nested: self,
            always_singular,
        }
    }

    pub fn singular(&self) -> RunsSingular {
        RunsSingular(self.0)
    }
}
pub struct RunsSingular(pub f64);

impl Display for RunsSingular {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 < 0.0 {
            write!(f, "{} Unrun", -self.0)
        } else {
            write!(f, "{} Run", self.0)
        }
    }
}

pub struct RunsWithUnrunsAlwaysPlural(pub f64);

impl Display for RunsWithUnrunsAlwaysPlural {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == 1.0 {
            write!(f, "1 Run")
        } else if self.0 >= 0.0 {
            write!(f, "{} Runs", self.0)
        } else {
            write!(f, "{} Unruns", -self.0)
        }
    }
}

impl RunsWithUnrunsAlwaysPlural {
    pub fn singular_if(
        self,
        always_singular: bool,
    ) -> RunsWithUnrunsAlwaysPluralAndSingularOverride {
        RunsWithUnrunsAlwaysPluralAndSingularOverride {
            nested: self,
            always_singular,
        }
    }

    // Note: This will erase the "unruns always plural" fact and return singular unruns anyway
    pub fn singular(&self) -> RunsSingular {
        RunsSingular(self.0)
    }
}

pub struct RunsWithSingularOverride {
    pub nested: Runs,
    pub always_singular: bool,
}

impl Display for RunsWithSingularOverride {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.always_singular {
            self.nested.singular().fmt(f)
        } else {
            self.nested.fmt(f)
        }
    }
}

pub struct RunsWithUnrunsAlwaysPluralAndSingularOverride {
    pub nested: RunsWithUnrunsAlwaysPlural,
    pub always_singular: bool,
}

impl Display for RunsWithUnrunsAlwaysPluralAndSingularOverride {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Singular override beats unruns always plural
        if self.always_singular {
            self.nested.singular().fmt(f)
        } else {
            self.nested.fmt(f)
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

// Like `Runs`, but for whole number runs. It accepts a signed int because I use that everywhere to
// facilitate interoperability
pub struct WholeRuns(pub i64);

impl Display for WholeRuns {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.0 == 1 {
            write!(f, "1 Run")
        } else {
            write!(f, "{} Runs", self.0)
        }
    }
}
