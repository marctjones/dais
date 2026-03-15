/// AT Protocol repo sync
///
/// TODO: Migrate from workers/pds/

use crate::{CoreResult, CoreError};

pub async fn handle_sync() -> CoreResult<()> {
    // TODO: Implement AT Protocol sync
    Err(CoreError::Internal("Not implemented".to_string()))
}
