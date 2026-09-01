pub(crate) fn anthropic_oauth_route_availability(model: &str) -> (bool, String) {
    if model.ends_with("[1m]") && !crate::usage::has_extra_usage() {
        (false, "requires extra usage".to_string())
    } else if model.contains("opus") && !crate::alphacode_base::auth::claude::is_max_subscription() {
        (false, "requires Max subscription".to_string())
    } else {
        (true, String::new())
    }
}

pub(crate) fn anthropic_api_key_route_availability(model: &str) -> (bool, String) {
    if model.ends_with("[1m]") && !crate::usage::has_extra_usage() {
        (false, "requires extra usage".to_string())
    } else {
        (true, String::new())
    }
}
