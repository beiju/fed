use std::mem;

pub struct PeekableWithLogging<IterT: Iterator> {
    iter: IterT,
    peeked: Option<IterT::Item>,
    log: Vec<IterT::Item>,
}

impl<IterT: Iterator> PeekableWithLogging<IterT> where IterT::Item: Clone {
    pub fn new(iter: IterT) -> Self {
        Self {
            iter,
            peeked: None,
            log: Vec::new(),
        }
    }

    fn raw_next(&mut self) -> Option<IterT::Item> {
        if let Some(item) = self.peeked.take() {
            Some(item)
        } else {
            self.iter.next()
        }
    }

    pub fn next(&mut self) -> Option<IterT::Item> {
        let item = self.raw_next();
        if let Some(item) = &item {
            // It's probably possible to make this work without clone by returning a reference to the item within
            // self.log, but I don't want to do that work now.
            self.log.push(item.clone())
        }
        item
    }

    pub fn peek(&mut self) -> Option<&IterT::Item> {
        if self.peeked.is_none() {
            // Note that peeked is not necessarily Some after this, if next() returned None
            self.peeked = self.iter.next()
        }
        self.peeked.as_ref()
    }

    pub fn log(&self) -> &Vec<IterT::Item> {
        &self.log
    }

    pub fn take_log(&mut self) -> Vec<IterT::Item> {
        // Replaces the referenced variable with the type's default, which for Vec is an empty vec
        mem::take(&mut self.log)
    }
}

pub trait MakePeekableWithLogging {
    type IterT: Iterator;

    fn peekable_with_logging(self) -> PeekableWithLogging<Self::IterT>;
}

impl<T> MakePeekableWithLogging for T where T: Iterator, T::Item: Clone {
    type IterT = T;

    fn peekable_with_logging(self) -> PeekableWithLogging<Self::IterT> {
        PeekableWithLogging::new(self)
    }
}
