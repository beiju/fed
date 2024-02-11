use std::collections::VecDeque;
use std::mem;
use itertools::Itertools;

pub struct PeekableWithLogging<IterT: Iterator> {
    iter: IterT,
    peeked: VecDeque<IterT::Item>,
    log: Vec<IterT::Item>,
}

impl<IterT: Iterator> PeekableWithLogging<IterT> where IterT::Item: Clone {
    pub fn new(iter: IterT) -> Self {
        Self {
            iter,
            peeked: VecDeque::new(),
            log: Vec::new(),
        }
    }

    fn raw_next(&mut self) -> Option<IterT::Item> {
        if let Some(item) = self.peeked.pop_front() {
            Some(item)
        } else {
            self.iter.next()
        }
    }

    fn yield_item(&mut self, item: Option<IterT::Item>) -> Option<IterT::Item> {
        if let Some(item) = &item {
            // It's probably possible to make this work without clone by returning a reference to
            // the item within self.log, but I don't want to do that work now.
            self.log.push(item.clone())
        }
        item
    }

    pub fn next(&mut self) -> Option<IterT::Item> {
        let item = self.raw_next();
        self.yield_item(item)
    }

    pub fn peek(&mut self) -> Option<&IterT::Item> {
        if self.peeked.is_empty() {
            if let Some(next) = self.iter.next() {
                self.peeked.push_front(next);
            }
        }
        self.peeked.front()
    }

    pub fn log(&self) -> &Vec<IterT::Item> {
        &self.log
    }

    pub fn take_log(&mut self) -> Vec<IterT::Item> {
        // Replaces the referenced variable with the type's default, which for Vec is an empty vec
        mem::take(&mut self.log)
    }

    // Look forward in the iterator for an element that matches the predicate. If one is found,
    // return it right away (this has the effect of changing the order elements are returned).
    // Elements for which pred returns false are retained and will still be yielded in future calls
    // to next() or extract_next_match()
    pub fn extract_next_match(&mut self, mut pred: impl FnMut(&IterT::Item) -> bool) -> Option<IterT::Item> {
        // First, try everything that's currently peeked
        if let Some((index, _)) = self.peeked.iter().find_position(|e| pred(e)) {
            // This should always be Some, but no point unwrapping it and re-wrapping it
            let item = self.peeked.remove(index);
            return self.yield_item(item)
        }

        // Otherwise, scroll forward through the iterator
        while let Some(item) = self.iter.next() {
            if pred(&item) {
                return self.yield_item(Some(item))
            }
            self.peeked.push_back(item)
        }

        None
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
