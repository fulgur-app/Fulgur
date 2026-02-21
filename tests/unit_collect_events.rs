/// Unit tests for the collect_events utility function
///
/// The collect_events function is a generic utility that drains all pending events
/// from an mpsc::Receiver without blocking, returning them as a Vec.
use fulgur::fulgur::utils::utilities::collect_events;
use std::sync::mpsc;

#[test]
fn test_collect_events_none_receiver() {
    let receiver: Option<mpsc::Receiver<i32>> = None;
    let events = collect_events(&receiver);
    assert!(events.is_empty());
}

#[test]
fn test_collect_events_empty_channel() {
    let (_tx, rx) = mpsc::channel::<i32>();
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert!(events.is_empty());
}

#[test]
fn test_collect_events_single_event() {
    let (tx, rx) = mpsc::channel();
    tx.send(42).unwrap();
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], 42);
}

#[test]
fn test_collect_events_multiple_events() {
    let (tx, rx) = mpsc::channel();
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events.len(), 3);
    assert_eq!(events, vec![1, 2, 3]);
}

#[test]
fn test_collect_events_drains_all() {
    let (tx, rx) = mpsc::channel();
    // Send many events
    for i in 0..100 {
        tx.send(i).unwrap();
    }
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events.len(), 100);
    assert_eq!(events[0], 0);
    assert_eq!(events[99], 99);
}

#[test]
fn test_collect_events_with_string() {
    let (tx, rx) = mpsc::channel();
    tx.send("hello".to_string()).unwrap();
    tx.send("world".to_string()).unwrap();
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0], "hello");
    assert_eq!(events[1], "world");
}

#[test]
fn test_collect_events_preserves_order() {
    let (tx, rx) = mpsc::channel();
    for i in 1..=10 {
        tx.send(i).unwrap();
    }
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
}

#[test]
fn test_collect_events_different_types() {
    // Test with bool
    let (tx, rx) = mpsc::channel();
    tx.send(true).unwrap();
    tx.send(false).unwrap();
    let receiver = Some(rx);
    let events = collect_events(&receiver);
    assert_eq!(events, vec![true, false]);
}
