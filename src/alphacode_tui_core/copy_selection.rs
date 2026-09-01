#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopySelectionPane {
    Chat,
    SidePane,
    /// The prompt composer (input box) where the user types the next message.
    Input,
}

impl CopySelectionPane {
    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::SidePane => "Side pane",
            Self::Input => "Input",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopySelectionPoint {
    pub pane: CopySelectionPane,
    pub abs_line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopySelectionRange {
    pub start: CopySelectionPoint,
    pub end: CopySelectionPoint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopySelectionStatus {
    pub pane: CopySelectionPane,
    pub has_action: bool,
    pub selected_chars: usize,
    pub selected_lines: usize,
    pub dragging: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_labels_match_ui_copy() {
        assert_eq!(CopySelectionPane::Chat.label(), "Chat");
        assert_eq!(CopySelectionPane::SidePane.label(), "Side pane");
        assert_eq!(CopySelectionPane::Input.label(), "Input");
    }
}
