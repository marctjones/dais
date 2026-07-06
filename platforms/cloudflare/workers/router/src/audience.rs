pub(crate) fn normalize_audience_group_type(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "private_group" | "private" | "group" | "community" => "private_group",
        _ => "audience",
    }
}

pub(crate) fn normalize_audience_membership_visibility(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "members" | "member" | "group" => "members",
        "public" | "visible" => "public",
        _ => "private",
    }
}

pub(crate) fn normalize_audience_posting_policy(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "members" | "member" | "group" => "members",
        _ => "owner",
    }
}

pub(crate) fn audience_group_purpose_label(group_type: &str) -> &'static str {
    match normalize_audience_group_type(group_type) {
        "private_group" => "Private group",
        _ => "Audience list",
    }
}

pub(crate) fn audience_membership_label(membership_visibility: &str) -> &'static str {
    match normalize_audience_membership_visibility(membership_visibility) {
        "members" => "Membership visible to members",
        "public" => "Membership public",
        _ => "Membership private",
    }
}
