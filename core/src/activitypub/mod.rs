/// ActivityPub protocol implementation
///
/// This module contains platform-agnostic ActivityPub logic that will be
/// migrated from the existing workers.
///
/// TODO: Migrate from workers/ to this module

pub mod types;
pub mod actor;
pub mod inbox;
pub mod outbox;
pub mod delivery;
pub mod signatures;

pub use types::*;
pub use actor::*;
pub use signatures::{
    HttpSignature, sign_request, verify_request, verify_digest,
    fetch_actor_public_key, build_signing_string,
};
pub use inbox::{
    process_inbox_activity, ContentModerator, ModerationResult,
    is_blocked, create_notification,
};
pub use outbox::{
    get_outbox_posts, get_post, get_post_interactions,
    Post, PostInteractions, Reply, Interaction,
};
pub use delivery::{
    get_follower_inboxes, deliver_to_inbox, create_follower_deliveries,
    update_delivery_status, DeliveryJob,
};
