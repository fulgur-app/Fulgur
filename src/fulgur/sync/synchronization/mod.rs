mod begin;
mod bootstrap;
mod error;
mod limits;
mod ping;
mod shared_files;
mod version;

pub use begin::{InitialSyncOutcome, initial_synchronization, list_pending_share_ids_v2};
pub use bootstrap::{
    begin_synchronization, perform_initial_synchronization,
    perform_initial_synchronization_with_progress, record_fulgurant_version,
    record_server_min_fulgur_version, set_sync_server_connection_status,
};
pub use error::{SynchronizationError, SynchronizationStatus, handle_ureq_error};
pub use limits::{
    MAX_HTTP_DEVICES_RESPONSE_BYTES, MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES,
    MAX_HTTP_SMALL_RESPONSE_BYTES, max_http_bulk_shares_response_bytes,
    max_http_single_share_response_bytes, resolve_server_max_file_size, store_server_max_file_size,
};
pub use ping::{perform_ping_with_progress, ping_server};
pub use version::{
    FULGURANT_VERSION_HEADER, FULGURANT_VERSION_WITHOUT_HEADER, RECOMMENDED_FULGURANT_VERSION,
    VersionCompatibility, compare_required_version, version_supports_per_id_fetch,
    version_supports_v2_share_flow,
};
