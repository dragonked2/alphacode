//! Model labels and usage metadata for Alphax Free.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::alphacode_provider_metadata::free_pool::{
    ALPHAX_FREE_DISPLAY_NAME, is_free_tier_model_id, merge_pool,
};
use crate::alphacode_provider_metadata::route_cloak;
// The curated floor is only asserted against, never used at runtime: rotation
// reads the merged pool, which is a superset once a refresh has run.
#[cfg(test)]
use crate::alphacode_provider_metadata::free_pool::curated_free_models;

/// Serialize tests that touch the process-wide pool metadata.
///
/// The pool metadata is process-global, and tests in other modules
/// (the display-name mapping in the TUI) exercise the same global state. A
/// single shared lock keeps those test runs from racing each other.
#[cfg(test)]
pub(crate) static POOL_STATE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn lock_pool_state_for_tests() -> std::sync::MutexGuard<'static, ()> {
    let guard = POOL_STATE_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Clearing under the lock gives every holder the same clean slate as the
    // local tests' `reset()`.
    if let Ok(mut pool) = refreshed_pool().lock() {
        pool.clear();
        FREE_POOL_REVISION.fetch_add(1, Ordering::Release);
    }
    guard
}

/// Models discovered as free by the background catalog refresh, beyond the
/// curated floor.
static FREE_POOL_REVISION: AtomicU64 = AtomicU64::new(0);

pub(crate) fn free_pool_revision() -> u64 {
    FREE_POOL_REVISION.load(Ordering::Acquire)
}

fn refreshed_pool() -> &'static Mutex<Vec<String>> {
    static POOL: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(Vec::new()))
}

/// Merge newly discovered free models into the live pool.
///
/// Called by the background refresh. A poisoned lock is treated as "no new
/// models" rather than propagated: losing a pool update is not worth failing a
/// session over, and the curated floor still applies.
pub fn absorb_discovered_free_models(ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    let Ok(mut pool) = refreshed_pool().lock() else {
        return;
    };
    for id in ids {
        if is_free_tier_model_id(&id) && !pool.contains(&id) {
            pool.push(id);
            FREE_POOL_REVISION.fetch_add(1, Ordering::Release);
        }
    }
}

/// The full effective pool: curated floor first, then anything the refresh
/// found. Curated entries are never dropped, so a failed or empty refresh
/// cannot shrink the pool below the set known to work.
pub fn effective_free_pool() -> Vec<String> {
    let discovered = refreshed_pool()
        .lock()
        .map(|p| p.clone())
        .unwrap_or_default();
    merge_pool(&discovered)
}

/// The user-visible product name for a model that belongs to the Alphax Free
/// pool, or `None` when the model is not part of it.
///
/// Membership is decided by the pool itself — the curated floor plus whatever
/// the gateway refresh discovered — and deliberately *not* by the `:free`
/// suffix. Other providers publish their own `:free` ids (Token Harbor's
/// catalog, for one) that are not Alphax Free; labelling those with the product
/// name hid the real model the user selected, so the picker and header read
/// "Alphax Free" for a provider the user never logged into. Only a model the
/// Alphax Free gateway actually serves may present as
/// [`ALPHAX_FREE_DISPLAY_NAME`].
pub fn free_pool_display_name(model: &str) -> Option<&'static str> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    // Rendering and filtering call this for every model. Do not clone and
    // deduplicate the entire pool to answer one membership query.
    let is_member = crate::alphacode_provider_metadata::free_pool::curated_free_models()
        .contains(&model)
        || refreshed_pool()
            .lock()
            .is_ok_and(|pool| pool.iter().any(|id| id == model));
    is_member.then_some(ALPHAX_FREE_DISPLAY_NAME)
}

/// Inverse of [`free_pool_display_name`]: turn the product label a user-facing
/// surface shows back into the model id that should be sent to the gateway.
///
/// Display surfaces intentionally collapse the pool onto one row labelled
/// "Alphax Free", and those surfaces then feed the user's choice back to the
/// session as if it were a model id. Without this resolver that round trip
/// sends the *label* upstream, which is not a routable model, so picking the
/// pool row from the desktop picker failed. Resolving to the automatic routing
/// alias is the correct mapping: it is the curated floor every other pool
/// member falls back to, and it keeps the gateway free to pick whichever
/// upstream serves the turn.
///
/// Only the exact product label resolves. A label with decoration such as the
/// picker's "(automatic)" suffix, or an ordinary model id, passes through
/// unchanged — this must never rewrite a real model name.
pub fn resolve_free_pool_model_selection(model: &str) -> Option<&'static str> {
    let model = model.trim();
    // The picker decorates the automatic row, so accept an optional trailing
    // marker when matching. Nothing else is stripped: a suffix test on
    // arbitrary text would hijack ids that merely start the same way.
    let base = model
        .strip_suffix("(automatic)")
        .map(str::trim_end)
        .unwrap_or(model);
    if !base.eq_ignore_ascii_case(ALPHAX_FREE_DISPLAY_NAME) {
        return None;
    }
    Some(route_cloak::leaked_free_route_default_model())
}

/// The model-name segment of a free-pool member id, for picker rows that list
/// concrete pool members, or `None` when the label does not apply.
///
/// When discovery lands, the Alphax Free picker gains a row per pool member so
/// a favourite can be pinned directly. Those rows are labelled only with the
/// member's model-name segment — everything before the `/` is a gateway routing
/// namespace and the `:free` marker is the gateway's tier tag, and neither is
/// ours to advertise. The automatic routing alias is excluded: it already
/// presents under its product name (`free_model_display_name` collapses to
/// "Alphax Free"), which is what the default row should keep showing.
///
/// Membership is the same pool-backed check [`free_pool_display_name`] uses, so
/// a `:free` id from another provider's catalog never picks up this label.
/// Callers prettify the returned segment with their own display formatter.
pub fn free_pool_picker_model_part(model: &str) -> Option<String> {
    let model = model.trim();
    if model.is_empty() || model == route_cloak::free_route_default_model() {
        return None;
    }
    let is_member = free_pool_display_name(model).is_some();
    if !is_member {
        return None;
    }
    // Strip the routing namespace (the id's first segment) and the tier marker.
    let name_part = model.rsplit('/').next()?;
    if name_part.is_empty() {
        return None;
    }
    let name_part = name_part.strip_suffix(":free").unwrap_or(name_part);
    if name_part.is_empty() {
        return None;
    }
    Some(name_part.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_provider_metadata::route_cloak;

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        super::lock_pool_state_for_tests()
    }

    /// Tests share a process-wide quarantine map, so each one must start from a
    /// clean slate or they will see each other's quarantines. The test lock
    /// keeps that reset and each test's subsequent operations atomic with
    /// respect to the other tests in this module.
    fn reset() {
        if let Ok(mut pool) = refreshed_pool().lock() {
            pool.clear();
            FREE_POOL_REVISION.fetch_add(1, Ordering::Release);
        }
    }

    /// The rotation floor is one alias; tests seed synthetic pool members
    /// through the same absorb path the background refresh uses.
    fn seed_pool(models: &[&str]) {
        absorb_discovered_free_models(models.iter().map(|m| m.to_string()).collect());
    }

    #[test]
    fn curated_pool_is_the_rotation_floor() {
        let _guard = test_lock();
        reset();
        let pool = effective_free_pool();
        assert!(pool.len() >= curated_free_models().len());
        for curated in curated_free_models() {
            assert!(
                pool.contains(&curated.to_string()),
                "floor `{curated}` missing from pool"
            );
        }
    }

    #[test]
    fn only_pool_members_present_as_alphax_free() {
        let _guard = test_lock();
        reset();
        seed_pool(&["synthetic/pool-a:free"]);
        // The floor alias carries the product name.
        assert_eq!(
            free_pool_display_name(&route_cloak::free_route_default_model()),
            Some(ALPHAX_FREE_DISPLAY_NAME)
        );
        assert_eq!(
            free_pool_display_name("synthetic/pool-a:free"),
            Some(ALPHAX_FREE_DISPLAY_NAME)
        );
        // A `:free` id from a different provider is not in the pool and must
        // keep its real name rather than borrowing the Alphax Free product name.
        assert_eq!(free_pool_display_name("tokenharbor/qwen3-max:free"), None);
        assert_eq!(free_pool_display_name("openai/gpt-5.4"), None);
        assert_eq!(free_pool_display_name(""), None);
    }

    #[test]
    fn the_product_label_resolves_back_to_the_routing_alias() {
        let _guard = test_lock();
        reset();
        // A surface that shows the label must be able to hand the choice back
        // as a routable id, otherwise selecting the pool row sends our own UI
        // copy upstream as a model name.
        assert_eq!(
            resolve_free_pool_model_selection(ALPHAX_FREE_DISPLAY_NAME),
            Some(route_cloak::free_route_default_model().as_str())
        );
        // The picker decorates the automatic row; the marker must not defeat
        // the lookup.
        assert_eq!(
            resolve_free_pool_model_selection("Alphax Free (automatic)"),
            Some(route_cloak::free_route_default_model().as_str())
        );
        assert_eq!(
            resolve_free_pool_model_selection("  alphax free  "),
            Some(route_cloak::free_route_default_model().as_str())
        );
        // Real model ids are never rewritten by this helper.
        let alias = route_cloak::free_route_default_model();
        assert_eq!(resolve_free_pool_model_selection(&alias), None);
        assert_eq!(
            resolve_free_pool_model_selection("synthetic/pool-a:free"),
            None
        );
        assert_eq!(resolve_free_pool_model_selection("openai/gpt-5.4"), None);
        // Prefix-ish text must not be hijacked into the pool label.
        assert_eq!(resolve_free_pool_model_selection("Alphax Free Turbo"), None);
        assert_eq!(resolve_free_pool_model_selection(""), None);
    }

    #[test]
    fn display_name_and_selection_resolution_round_trip() {
        let _guard = test_lock();
        reset();
        seed_pool(&["synthetic/pool-a:free"]);
        let alias = route_cloak::free_route_default_model();
        // forward(alias) == label, and inverse(label) == alias.
        let label = free_pool_display_name(&alias).expect("alias is a pool member");
        assert_eq!(
            resolve_free_pool_model_selection(label),
            Some(alias.as_str())
        );
    }

    #[test]
    fn picker_model_part_strips_the_gateway_routing_details() {
        let _guard = test_lock();
        reset();
        seed_pool(&["synthetic/pool-a:free", "synthetic/pool-b:legacy:free"]);
        // Only the model-name segment survives: the first part of the id is a
        // gateway routing namespace and the `:free` marker is its tier tag.
        assert_eq!(
            free_pool_picker_model_part("synthetic/pool-a:free"),
            Some("pool-a".to_string())
        );
        assert_eq!(
            free_pool_picker_model_part("synthetic/pool-b:legacy:free"),
            Some("pool-b:legacy".to_string())
        );
        // The automatic routing alias keeps its product label, so it is excluded.
        assert_eq!(
            free_pool_picker_model_part(&route_cloak::free_route_default_model()),
            None
        );
        // Non-members return None regardless of shape, so ids from other
        // providers are never draped in these labels.
        assert_eq!(
            free_pool_picker_model_part("tokenharbor/qwen3-max:free"),
            None
        );
        assert_eq!(free_pool_picker_model_part("openai/gpt-5.4"), None);
        assert_eq!(free_pool_picker_model_part(""), None);
        assert_eq!(free_pool_picker_model_part(":free"), None);
    }

    #[test]
    fn discovered_models_join_the_pool() {
        let _guard = test_lock();
        reset();
        absorb_discovered_free_models(vec![
            "brand-new/model:free".to_string(),
            "paid/gpt-5.4".to_string(),
        ]);
        let pool = effective_free_pool();
        assert!(pool.contains(&"brand-new/model:free".to_string()));
        assert!(
            !pool.contains(&"paid/gpt-5.4".to_string()),
            "a paid model must never enter the free pool"
        );
    }

    #[test]
    fn absorbing_an_empty_refresh_leaves_the_pool_intact() {
        let _guard = test_lock();
        reset();
        let before = effective_free_pool();
        absorb_discovered_free_models(Vec::new());
        assert_eq!(before, effective_free_pool());
    }
}
