/// Collapsible swarm notification content.
///
/// When a swarm message/report arrives with a sender-provided `tldr`, the TUI
/// stores the full body in the transcript `DisplayMessage` but renders only
/// the tldr line plus an expand hint. The collapsed/expanded state is encoded
/// in the message content itself (behind zero-width markers), so toggling
/// rewrites the content and every content-hash-keyed render cache invalidates
/// naturally.
///
/// Encoded layout: `{MARKER}{state}{MARKER}{tldr}\n{body}` where `state` is
/// `collapsed` or `expanded`. The markers are zero-width so even if the raw
/// content leaks somewhere (copy, logs), it degrades to readable text.
const MARKER: &str = "\u{200b}\u{200b}";
const STATE_COLLAPSED: &str = "swarm-tldr:collapsed";
const STATE_EXPANDED: &str = "swarm-tldr:expanded";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollapsibleSwarmContent<'a> {
    pub tldr: &'a str,
    pub body: &'a str,
    pub expanded: bool,
}

/// Encode a tldr + full body as collapsed swarm content.
pub fn encode_collapsible_swarm_content(tldr: &str, body: &str) -> String {
    format!("{MARKER}{STATE_COLLAPSED}{MARKER}{tldr}\n{body}")
}

/// Parse collapsible swarm content. Returns `None` for plain content.
pub fn parse_collapsible_swarm_content(content: &str) -> Option<CollapsibleSwarmContent<'_>> {
    let rest = content.strip_prefix(MARKER)?;
    let (state, rest) = rest.split_once(MARKER)?;
    let expanded = match state {
        STATE_COLLAPSED => false,
        STATE_EXPANDED => true,
        _ => return None,
    };
    let (tldr, body) = rest.split_once('\n').unwrap_or((rest, ""));
    Some(CollapsibleSwarmContent {
        tldr,
        body,
        expanded,
    })
}

/// Toggle the collapsed/expanded state of encoded content. Returns `None`
/// when the content is not collapsible.
pub fn toggle_collapsible_swarm_content(content: &str) -> Option<String> {
    let parsed = parse_collapsible_swarm_content(content)?;
    let state = if parsed.expanded {
        STATE_COLLAPSED
    } else {
        STATE_EXPANDED
    };
    Some(format!(
        "{MARKER}{state}{MARKER}{}\n{}",
        parsed.tldr, parsed.body
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_collapsed_then_toggle() {
        let encoded = encode_collapsible_swarm_content("did the thing", "long\nbody\nhere");
        let parsed = parse_collapsible_swarm_content(&encoded).expect("collapsible");
        assert_eq!(parsed.tldr, "did the thing");
        assert_eq!(parsed.body, "long\nbody\nhere");
        assert!(!parsed.expanded);

        let expanded = toggle_collapsible_swarm_content(&encoded).expect("toggle");
        let parsed = parse_collapsible_swarm_content(&expanded).expect("collapsible");
        assert!(parsed.expanded);
        assert_eq!(parsed.body, "long\nbody\nhere");

        let collapsed_again = toggle_collapsible_swarm_content(&expanded).expect("toggle");
        assert_eq!(collapsed_again, encoded);
    }

    #[test]
    fn plain_content_is_not_collapsible() {
        assert!(parse_collapsible_swarm_content("just a message").is_none());
        assert!(toggle_collapsible_swarm_content("just a message").is_none());
    }

    #[test]
    fn empty_body_parses() {
        let encoded = encode_collapsible_swarm_content("tldr only", "");
        let parsed = parse_collapsible_swarm_content(&encoded).expect("collapsible");
        assert_eq!(parsed.tldr, "tldr only");
        assert_eq!(parsed.body, "");
    }
}
