//! Character-count flow control against the firmware's 127-byte ring (B3, B4).

use std::collections::VecDeque;

/// Bytes WebControl lets the controller hold; it counts the trailing space and `\n` as stored.
pub const BUFFER_SIZE: i64 = 126;

/// A line written and not yet acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sent {
    pub id: u64,
    pub len: i64,
    /// The file line it came from, so a halt can rewind to it.
    pub index: Option<usize>,
}

/// Unacknowledged lines, newest first, and the space they leave in the controller's buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accounting {
    unacked: VecDeque<Sent>,
    space: i64,
    next_id: u64,
}

impl Default for Accounting {
    fn default() -> Self {
        Accounting { unacked: VecDeque::new(), space: BUFFER_SIZE, next_id: 0 }
    }
}

impl Accounting {
    fn record(&mut self, len: usize, index: Option<usize>) -> Sent {
        self.next_id += 1;
        let sent = Sent { id: self.next_id, len: len as i64, index };
        self.space -= sent.len;
        sent
    }

    /// A line through the buffer, with its `\n`.
    pub fn sent(&mut self, len: usize, index: Option<usize>) -> Sent {
        let sent = self.record(len, index);
        self.unacked.push_front(sent);
        sent
    }

    /// `~` is acknowledged at once, ahead of every line still in the buffer.
    pub fn realtime_sent(&mut self, len: usize) -> Sent {
        let sent = self.record(len, None);
        self.unacked.push_back(sent);
        sent
    }

    /// An `ok` or `error:` frees the oldest line.
    pub fn acked(&mut self) -> Option<Sent> {
        let sent = self.unacked.pop_back()?;
        self.space += sent.len;
        Some(sent)
    }

    pub fn clear(&mut self) {
        self.unacked.clear();
        self.space = BUFFER_SIZE;
    }

    pub fn space(&self) -> i64 {
        self.space
    }

    /// Nothing written is still waiting for its ack.
    pub fn is_empty(&self) -> bool {
        self.space == BUFFER_SIZE
    }

    /// The oldest file line the controller dropped on a halt.
    pub fn oldest_index(&self) -> Option<usize> {
        self.unacked.iter().filter_map(|s| s.index).min()
    }
}
