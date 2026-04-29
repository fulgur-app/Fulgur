use parking_lot::Mutex;
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::AtomicBool,
        mpsc::{Receiver, Sender},
    },
    thread,
    time::Instant,
};

/// Server-Sent Events state for sync functionality
pub struct SseState {
    pub sse_events: Option<Receiver<SseEvent>>,
    pub sse_event_tx: Option<Sender<SseEvent>>,
    pub sse_shutdown_flag: Option<Arc<AtomicBool>>,
    pub last_sse_event: Option<Instant>,
    /// Handle to the current SSE background thread for lifecycle tracking.
    pub sse_thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl Default for SseState {
    /// Create a new SseState with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self`: A new SseState
    fn default() -> Self {
        Self::new()
    }
}

impl SseState {
    /// Create a new SseState with all fields initialized to None
    ///
    /// ### Returns
    /// `Self`: a new SseState
    pub fn new() -> Self {
        Self {
            sse_events: None,
            sse_event_tx: None,
            sse_shutdown_flag: None,
            last_sse_event: None,
            sse_thread_handle: Arc::new(Mutex::new(None)),
        }
    }
}

/// Heartbeat event data sent by SSE to keep connection alive
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct HeartbeatData {
    pub timestamp: String,
}

/// Lightweight share notification carried over SSE.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ShareNotification {
    pub share_id: String,
}

/// SSE Event types that can be received from the server
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Heartbeat event to keep connection alive (sent every ~30s)
    Heartbeat { timestamp: String },
    /// File share notification with full share details
    ShareAvailable(ShareNotification),
    /// Error event for connection or parsing errors
    Error(String),
}

impl SseEvent {
    /// Parse an SSE event from the event type and data
    ///
    /// ### Arguments
    /// - `event_type`: The SSE event type (e.g., "heartbeat", "share_available")
    /// - `data`: The JSON data for the event
    ///
    /// ### Returns
    /// - `SseEvent`: The parsed event
    pub fn parse(event_type: &str, data: &str) -> Self {
        match event_type {
            "heartbeat" => match serde_json::from_str::<HeartbeatData>(data) {
                Ok(hb) => SseEvent::Heartbeat {
                    timestamp: hb.timestamp,
                },
                Err(e) => {
                    log::warn!("Failed to parse heartbeat: {}", e);
                    SseEvent::Heartbeat {
                        timestamp: String::new(),
                    }
                }
            },
            "share_available" => match serde_json::from_str::<ShareNotification>(data) {
                Ok(notification) => SseEvent::ShareAvailable(notification),
                Err(e) => {
                    log::error!("Failed to parse share notification: {}", e);
                    SseEvent::Error(format!("Invalid share notification: {}", e))
                }
            },
            "" => {
                log::error!("Unknown event with no event type");
                SseEvent::Error("Unknown event with no event type".to_string())
            }
            _ => {
                log::warn!("Unknown SSE event type: {}", event_type);
                SseEvent::Error(format!("Unknown event type: {}", event_type))
            }
        }
    }
}
