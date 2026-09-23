use std::collections::BTreeMap;

/// Visual state for a session rectangle in the workspace map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceSessionVisualState {
    #[default]
    Idle,
    Running,
    Completed,
    Waiting,
    Error,
    Detached,
}

/// A single session in a Niri-style horizontal workspace strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSessionTile {
    pub session_id: String,
    pub state: WorkspaceSessionVisualState,
}

impl WorkspaceSessionTile {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            state: WorkspaceSessionVisualState::Idle,
        }
    }

    pub fn with_state(session_id: impl Into<String>, state: WorkspaceSessionVisualState) -> Self {
        Self {
            session_id: session_id.into(),
            state,
        }
    }
}

/// A logical workspace row. Sessions are ordered left-to-right.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceRow {
    pub sessions: Vec<WorkspaceSessionTile>,
    /// Last focused session index within this row.
    pub last_focused: Option<usize>,
}

impl WorkspaceRow {
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn focused_index(&self) -> Option<usize> {
        let len = self.sessions.len();
        self.last_focused
            .filter(|idx| *idx < len)
            .or_else(|| (!self.sessions.is_empty()).then_some(0))
    }

    pub fn focus(&mut self, index: usize) -> bool {
        if index < self.sessions.len() {
            self.last_focused = Some(index);
            true
        } else {
            false
        }
    }

    /// Insert a session to the right of the currently focused session.
    /// If nothing is focused yet, append to the end.
    pub fn insert_right_of_focus(&mut self, tile: WorkspaceSessionTile) -> usize {
        let insert_at = self
            .focused_index()
            .map(|idx| (idx + 1).min(self.sessions.len()))
            .unwrap_or(self.sessions.len());
        self.sessions.insert(insert_at, tile);
        self.last_focused = Some(insert_at);
        insert_at
    }

    pub fn move_focus_left(&mut self) -> bool {
        let Some(current) = self.focused_index() else {
            return false;
        };
        if current == 0 {
            return false;
        }
        self.last_focused = Some(current - 1);
        true
    }

    pub fn move_focus_right(&mut self) -> bool {
        let Some(current) = self.focused_index() else {
            return false;
        };
        if current + 1 >= self.sessions.len() {
            return false;
        }
        self.last_focused = Some(current + 1);
        true
    }
}

/// A full Niri-style session workspace model.
///
/// Horizontal movement happens within a row. Vertical movement switches rows,
/// restoring the remembered focus for that workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceMapModel {
    rows: BTreeMap<i32, WorkspaceRow>,
    current_workspace: i32,
}

impl WorkspaceMapModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_workspace(&self) -> i32 {
        self.current_workspace
    }

    pub fn set_current_workspace(&mut self, workspace: i32) {
        self.current_workspace = workspace;
        self.rows.entry(workspace).or_default();
    }

    pub fn row(&self, workspace: i32) -> Option<&WorkspaceRow> {
        self.rows.get(&workspace)
    }

    pub fn row_mut(&mut self, workspace: i32) -> &mut WorkspaceRow {
        self.rows.entry(workspace).or_default()
    }

    pub fn current_row(&self) -> Option<&WorkspaceRow> {
        self.row(self.current_workspace)
    }

    pub fn current_row_mut(&mut self) -> &mut WorkspaceRow {
        self.row_mut(self.current_workspace)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.values().all(WorkspaceRow::is_empty)
    }

    pub fn add_session_to_current_workspace(&mut self, tile: WorkspaceSessionTile) -> (i32, usize) {
        let workspace = self.current_workspace;
        let index = self.current_row_mut().insert_right_of_focus(tile);
        (workspace, index)
    }

    pub fn focus_session_in_workspace(&mut self, workspace: i32, index: usize) -> bool {
        self.row_mut(workspace).focus(index)
    }

    pub fn locate_session(&self, session_id: &str) -> Option<(i32, usize)> {
        self.rows.iter().find_map(|(workspace, row)| {
            row.sessions
                .iter()
                .position(|tile| tile.session_id == session_id)
                .map(|index| (*workspace, index))
        })
    }

    pub fn focus_session_by_id(&mut self, session_id: &str) -> bool {
        let Some((workspace, index)) = self.locate_session(session_id) else {
            return false;
        };
        self.current_workspace = workspace;
        self.row_mut(workspace).focus(index)
    }

    pub fn current_focused_session_id(&self) -> Option<&str> {
        let row = self.current_row()?;
        let index = row.focused_index()?;
        row.sessions.get(index).map(|tile| tile.session_id.as_str())
    }

    pub fn set_row_sessions(
        &mut self,
        workspace: i32,
        sessions: Vec<WorkspaceSessionTile>,
        focused_index: Option<usize>,
    ) {
        let row = self.row_mut(workspace);
        row.sessions = sessions;
        row.last_focused = focused_index.filter(|idx| *idx < row.sessions.len());
    }

    pub fn insert_session_in_workspace(
        &mut self,
        workspace: i32,
        tile: WorkspaceSessionTile,
    ) -> usize {
        self.current_workspace = workspace;
        self.row_mut(workspace).insert_right_of_focus(tile)
    }

    pub fn focused_session_in_workspace(&self, workspace: i32) -> Option<&str> {
        let row = self.row(workspace)?;
        let index = row.focused_index()?;
        row.sessions.get(index).map(|tile| tile.session_id.as_str())
    }

    pub fn nearest_populated_workspace_above(&self) -> Option<i32> {
        self.rows
            .iter()
            .filter_map(|(workspace, row)| {
                (*workspace > self.current_workspace && !row.is_empty()).then_some(*workspace)
            })
            .min()
    }

    pub fn nearest_populated_workspace_below(&self) -> Option<i32> {
        self.rows
            .iter()
            .filter_map(|(workspace, row)| {
                (*workspace < self.current_workspace && !row.is_empty()).then_some(*workspace)
            })
            .max()
    }

    pub fn move_left(&mut self) -> bool {
        self.current_row_mut().move_focus_left()
    }

    pub fn move_right(&mut self) -> bool {
        self.current_row_mut().move_focus_right()
    }

    /// Move to the workspace above the current one, creating it if needed.
    pub fn move_up(&mut self) {
        self.current_workspace += 1;
        self.rows.entry(self.current_workspace).or_default();
    }

    /// Move to the workspace below the current one, creating it if needed.
    pub fn move_down(&mut self) {
        self.current_workspace -= 1;
        self.rows.entry(self.current_workspace).or_default();
    }

    pub fn populated_workspaces(&self) -> Vec<i32> {
        self.rows
            .iter()
            .filter_map(|(workspace, row)| (!row.is_empty()).then_some(*workspace))
            .collect()
    }

    /// Returns visible rows centered on the current workspace.
    ///
    /// Empty rows are omitted unless the row is the current workspace.
    pub fn visible_rows(&self, max_rows: usize) -> Vec<VisibleWorkspaceRow> {
        if max_rows == 0 {
            return Vec::new();
        }

        let mut ordered: Vec<i32> = self
            .rows
            .iter()
            .filter_map(|(workspace, row)| {
                if *workspace == self.current_workspace || !row.is_empty() {
                    Some(*workspace)
                } else {
                    None
                }
            })
            .collect();
        ordered.sort_unstable_by(|a, b| b.cmp(a));

        if ordered.is_empty() {
            ordered.push(self.current_workspace);
        }

        let current_pos = ordered
            .iter()
            .position(|workspace| *workspace == self.current_workspace)
            .unwrap_or(0);
        let half = max_rows / 2;
        let mut start = current_pos.saturating_sub(half);
        let end = (start + max_rows).min(ordered.len());
        if end - start < max_rows {
            start = end.saturating_sub(max_rows);
        }
        let slice = &ordered[start..end];

        slice
            .iter()
            .map(|workspace| {
                let row = self.rows.get(workspace).cloned().unwrap_or_default();
                VisibleWorkspaceRow {
                    workspace: *workspace,
                    is_current: *workspace == self.current_workspace,
                    focused_index: row.focused_index(),
                    sessions: row.sessions,
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleWorkspaceRow {
    pub workspace: i32,
    pub is_current: bool,
    pub focused_index: Option<usize>,
    pub sessions: Vec<WorkspaceSessionTile>,
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceMapModel, WorkspaceSessionTile, WorkspaceSessionVisualState};

    #[test]
    fn add_session_grows_current_row_to_the_right() {
        let mut map = WorkspaceMapModel::new();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("fox"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("bear"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("owl"));

        let row = map.current_row().expect("current row");
        let ids: Vec<_> = row.sessions.iter().map(|t| t.session_id.as_str()).collect();
        assert_eq!(ids, vec!["fox", "bear", "owl"]);
        assert_eq!(row.focused_index(), Some(2));
    }

    #[test]
    fn inserting_after_refocusing_places_new_session_to_the_right_of_focus() {
        let mut map = WorkspaceMapModel::new();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("fox"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("bear"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("owl"));

        assert!(map.focus_session_in_workspace(0, 0));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("ibis"));

        let row = map.current_row().expect("current row");
        let ids: Vec<_> = row.sessions.iter().map(|t| t.session_id.as_str()).collect();
        assert_eq!(ids, vec!["fox", "ibis", "bear", "owl"]);
        assert_eq!(row.focused_index(), Some(1));
    }

    #[test]
    fn moving_between_workspaces_remembers_last_focus_per_workspace() {
        let mut map = WorkspaceMapModel::new();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("fox"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("bear"));
        assert!(map.move_left());
        assert_eq!(
            map.current_row().and_then(|row| row.focused_index()),
            Some(0)
        );

        map.move_up();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("owl"));
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("ibis"));
        assert!(map.move_left());
        assert_eq!(map.current_workspace(), 1);
        assert_eq!(
            map.current_row().and_then(|row| row.focused_index()),
            Some(0)
        );

        map.move_down();
        assert_eq!(map.current_workspace(), 0);
        assert_eq!(
            map.current_row().and_then(|row| row.focused_index()),
            Some(0)
        );

        map.move_up();
        assert_eq!(map.current_workspace(), 1);
        assert_eq!(
            map.current_row().and_then(|row| row.focused_index()),
            Some(0)
        );
    }

    #[test]
    fn visible_rows_only_include_populated_rows_and_current_workspace() {
        let mut map = WorkspaceMapModel::new();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("fox"));
        map.move_up();
        map.move_up();
        map.add_session_to_current_workspace(WorkspaceSessionTile::new("owl"));
        map.move_down();

        let rows = map.visible_rows(5);
        let workspaces: Vec<_> = rows.iter().map(|row| row.workspace).collect();
        assert_eq!(workspaces, vec![2, 1, 0]);
        assert!(rows.iter().any(|row| row.workspace == 1 && row.is_current));
        assert!(
            rows.iter()
                .find(|row| row.workspace == 1)
                .expect("current workspace row")
                .sessions
                .is_empty()
        );
    }

    #[test]
    fn session_tiles_preserve_visual_state() {
        let mut map = WorkspaceMapModel::new();
        map.add_session_to_current_workspace(WorkspaceSessionTile::with_state(
            "fox",
            WorkspaceSessionVisualState::Running,
        ));
        let row = map.current_row().expect("current row");
        assert_eq!(row.sessions[0].state, WorkspaceSessionVisualState::Running);
    }
}
