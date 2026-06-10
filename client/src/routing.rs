use std::fmt;

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Protocol {
    #[value(name = "activitypub")]
    ActivityPub,
    Atproto,
    Both,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::ActivityPub => f.write_str("activitypub"),
            Protocol::Atproto => f.write_str("atproto"),
            Protocol::Both => f.write_str("both"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Visibility {
    Public,
    Unlisted,
    Followers,
    Direct,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => f.write_str("public"),
            Visibility::Unlisted => f.write_str("unlisted"),
            Visibility::Followers => f.write_str("followers"),
            Visibility::Direct => f.write_str("direct"),
        }
    }
}

pub fn effective_protocol(requested: Protocol, visibility: Visibility) -> Protocol {
    match (requested, visibility) {
        (Protocol::Both | Protocol::Atproto, Visibility::Followers | Visibility::Direct) => {
            Protocol::ActivityPub
        }
        (protocol, _) => protocol,
    }
}

#[cfg(test)]
mod tests {
    use super::{effective_protocol, Protocol, Visibility};

    #[test]
    fn public_posts_can_route_to_both_protocols() {
        assert_eq!(
            effective_protocol(Protocol::Both, Visibility::Public),
            Protocol::Both
        );
    }

    #[test]
    fn followers_posts_do_not_route_to_atproto() {
        assert_eq!(
            effective_protocol(Protocol::Both, Visibility::Followers),
            Protocol::ActivityPub
        );
        assert_eq!(
            effective_protocol(Protocol::Atproto, Visibility::Followers),
            Protocol::ActivityPub
        );
    }

    #[test]
    fn direct_posts_do_not_route_to_atproto() {
        assert_eq!(
            effective_protocol(Protocol::Both, Visibility::Direct),
            Protocol::ActivityPub
        );
    }
}
