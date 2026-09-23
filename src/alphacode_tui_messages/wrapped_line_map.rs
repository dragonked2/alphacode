#[derive(Clone, Copy, Debug)]
pub struct WrappedLineMap {
    pub raw_line: usize,
    pub start_col: usize,
    pub end_col: usize,
}
