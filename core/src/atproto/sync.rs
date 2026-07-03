/// AT Protocol repo sync
///
/// Router/PDS-owned sync operations are intentionally guarded until GitHub issue
/// #275 moves them into platform-agnostic core code.
use crate::{CoreError, CoreResult};

pub async fn handle_sync() -> CoreResult<()> {
    core_sync_migration_required("handle_sync")
}

pub(crate) fn core_sync_migration_required(operation: &str) -> CoreResult<()> {
    Err(CoreError::InvalidAtProto(format!(
        "{operation} is still handled by the Cloudflare router/PDS surface; migrate sync operations into dais-core under GitHub issue #275 before calling this core API"
    )))
}
