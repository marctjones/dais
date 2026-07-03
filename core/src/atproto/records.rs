/// AT Protocol record operations
///
/// Router/PDS-owned record operations are intentionally guarded until GitHub
/// issue #275 moves them into platform-agnostic core code.
use crate::{CoreError, CoreResult};

pub async fn create_record() -> CoreResult<()> {
    Err(CoreError::InvalidAtProto(
        "create_record is still handled by the Cloudflare router/PDS surface; migrate record operations into dais-core under GitHub issue #275 before calling this core API"
            .to_string(),
    ))
}
