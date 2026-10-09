//! Read state of GitHub comment threads (discussions, pull requests): which
//! messages were already read and how runs of read comments are grouped.

use chrono::{DateTime, Utc};

/// Comments as displayed: runs of 2+ fully read comments collapse behind a
/// single "Show N read comments" control.
#[derive(Clone, PartialEq, Debug)]
pub enum Segment<T> {
    Comment(Box<T>),
    ReadComments(Vec<T>),
}

pub fn is_read(created_at: DateTime<Utc>, last_read_at: Option<DateTime<Utc>>) -> bool {
    last_read_at.is_some_and(|last_read_at| created_at <= last_read_at)
}

/// Number of leading read messages (messages are oldest first).
pub fn read_prefix_len<T>(
    messages: &[T],
    created_at: impl Fn(&T) -> DateTime<Utc>,
    last_read_at: Option<DateTime<Utc>>,
) -> usize {
    messages
        .iter()
        .position(|message| !is_read(created_at(message), last_read_at))
        .unwrap_or(messages.len())
}

/// Group consecutive comments without unread messages.
pub fn segments<T: Clone>(comments: &[T], has_unread: impl Fn(&T) -> bool) -> Vec<Segment<T>> {
    let mut segments = Vec::new();
    let mut read_run: Vec<T> = Vec::new();
    let flush = |run: &mut Vec<T>, segments: &mut Vec<Segment<T>>| match run.len() {
        0 => {}
        1 => segments.push(Segment::Comment(Box::new(run.remove(0)))),
        _ => segments.push(Segment::ReadComments(std::mem::take(run))),
    };
    for comment in comments {
        if has_unread(comment) {
            flush(&mut read_run, &mut segments);
            segments.push(Segment::Comment(Box::new(comment.clone())));
        } else {
            read_run.push(comment.clone());
        }
    }
    flush(&mut read_run, &mut segments);
    segments
}
