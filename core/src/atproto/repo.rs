/// AT Protocol repository operations
///
/// Router/PDS-owned repo operations are intentionally guarded until GitHub issue
/// #275 moves them into platform-agnostic core code.
use crate::{CoreError, CoreResult};

pub async fn get_repo() -> CoreResult<()> {
    core_repo_migration_required("get_repo")
}

pub(crate) fn core_repo_migration_required(operation: &str) -> CoreResult<()> {
    Err(CoreError::InvalidAtProto(format!(
        "{operation} is still handled by the Cloudflare router/PDS surface; migrate repo operations into dais-core under GitHub issue #275 before calling this core API"
    )))
}
