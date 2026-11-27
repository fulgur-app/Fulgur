//! Code folding commands: fold or unfold a whole buffer, one nesting level, or the
//! block around the caret, keeping the caret on a visible line.

use super::EditorTab;
use gpui_kit::component::input::{EditorState, FoldRange, Position};
use gpui_kit::{App, Context, Entity, Window};

/// The deepest nesting level offered by the "Fold Level N" commands.
pub const MAX_FOLD_LEVEL: usize = 7;

/// A folding command applied to an editor buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FoldCommand {
    /// Fold every foldable block.
    FoldAll,
    /// Unfold every folded block.
    UnfoldAll,
    /// Fold the blocks at one nesting level (1 is the outermost) and unfold all others.
    FoldAtLevel(usize),
    /// Fold the innermost block around the caret, or unfold it when it is folded.
    ToggleAtCursor,
}

impl EditorTab {
    /// Get the content entity of this tab when its buffer can be folded.
    ///
    /// ### Returns
    /// - `Some(Entity<EditorState>)`: The buffer, folding is enabled on it
    /// - `None`: The tab is in large-file mode, which turns folding off
    #[must_use]
    pub fn foldable_content(&self) -> Option<Entity<EditorState>> {
        (!self.large_file).then(|| self.content.clone())
    }

    /// Get the deepest fold nesting level of this tab's buffer.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `usize`: The deepest level, `0` when nothing can be folded
    #[must_use]
    pub fn fold_depth(&self, cx: &App) -> usize {
        self.foldable_content()
            .map_or(0, |content| fold_depth(content.read(cx).fold_candidates()))
    }
}

/// Apply a folding command to a buffer, then move the caret out of any fold that now hides it.
///
/// ### Arguments
/// - `state`: The buffer to fold
/// - `command`: The folding command to apply
/// - `window`: The window context
/// - `cx`: The buffer's context
pub fn apply_fold_command(
    state: &mut EditorState,
    command: FoldCommand,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let caret = state.cursor_position();
    let caret_line = caret.line as usize;
    let hiding_fold_start = match command {
        FoldCommand::FoldAll => {
            state.fold_all(cx);
            enclosing_candidates(state.fold_candidates(), caret_line)
                .first()
                .map(|fold| fold.start_line)
        }
        FoldCommand::UnfoldAll => {
            state.unfold_all(cx);
            None
        }
        FoldCommand::FoldAtLevel(level) => {
            state.fold_at_level(level, cx);
            level.checked_sub(1).and_then(|index| {
                enclosing_candidates(state.fold_candidates(), caret_line)
                    .get(index)
                    .map(|fold| fold.start_line)
            })
        }
        FoldCommand::ToggleAtCursor => toggle_fold_at(state, caret, cx),
    };
    if let Some(start_line) = hiding_fold_start {
        state.set_cursor_position(line_position(start_line, caret.character), window, cx);
    }
}

/// Build an editor position from a buffer line index.
///
/// ### Arguments
/// - `line`: The 0-based buffer line
/// - `character`: The 0-based column, clamped to the line length by the editor
///
/// ### Returns
/// - `Position`: The position, its line saturated at `u32::MAX` (folding is off for
///   buffers anywhere near that many lines)
fn line_position(line: usize, character: u32) -> Position {
    Position {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        character,
    }
}

/// Fold the innermost block containing the caret, or unfold it when it is already folded.
///
/// ### Arguments
/// - `state`: The buffer to fold
/// - `caret`: The caret position
/// - `cx`: The buffer's context
///
/// ### Returns
/// - `Some(usize)`: The first line of the fold that now hides the caret
/// - `None`: The caret is still visible
fn toggle_fold_at(
    state: &mut EditorState,
    caret: Position,
    cx: &mut Context<EditorState>,
) -> Option<usize> {
    if state.unfold_at(caret, cx) {
        return None;
    }
    let caret_line = caret.line as usize;
    let target = innermost_candidate_at(state.fold_candidates(), caret_line)?;
    let first_hidden_line = line_position(target.start_line + 1, 0);
    if target.end_line > target.start_line + 1 && state.unfold_at(first_hidden_line, cx) {
        return None;
    }
    state.set_folded(target.start_line, true, cx);
    hides_line(&target, caret_line).then_some(target.start_line)
}

/// Check whether a folded block hides a buffer line.
///
/// ### Arguments
/// - `fold`: The folded block
/// - `line`: The 0-based buffer line
///
/// ### Returns
/// - `bool`: Whether the line lies strictly between the block's first and last line
fn hides_line(fold: &FoldRange, line: usize) -> bool {
    fold.start_line < line && line < fold.end_line
}

/// Get the fold candidates that would hide a line once folded, outermost first.
///
/// ### Arguments
/// - `candidates`: The fold candidates, sorted by start line
/// - `line`: The 0-based buffer line
///
/// ### Returns
/// - `Vec<FoldRange>`: The candidates strictly enclosing the line
fn enclosing_candidates(candidates: &[FoldRange], line: usize) -> Vec<FoldRange> {
    candidates
        .iter()
        .filter(|candidate| hides_line(candidate, line))
        .copied()
        .collect()
}

/// Get the innermost fold candidate whose range includes a line, its first and last line included.
///
/// ### Arguments
/// - `candidates`: The fold candidates, sorted by start line
/// - `line`: The 0-based buffer line
///
/// ### Returns
/// - `Some(FoldRange)`: The candidate starting last among those including the line
/// - `None`: No candidate includes the line
fn innermost_candidate_at(candidates: &[FoldRange], line: usize) -> Option<FoldRange> {
    candidates
        .iter()
        .rfind(|candidate| candidate.start_line <= line && line <= candidate.end_line)
        .copied()
}

/// Get the deepest nesting level among fold candidates.
///
/// ### Arguments
/// - `candidates`: The fold candidates, sorted by start line with unique starts
///
/// ### Returns
/// - `usize`: The deepest level, `0` when there is no candidate
fn fold_depth(candidates: &[FoldRange]) -> usize {
    let mut open_ends: Vec<usize> = Vec::new();
    let mut depth = 0;
    for candidate in candidates {
        while open_ends
            .last()
            .is_some_and(|&end| end < candidate.end_line)
        {
            open_ends.pop();
        }
        open_ends.push(candidate.end_line);
        depth = depth.max(open_ends.len());
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::{enclosing_candidates, fold_depth, innermost_candidate_at};
    use core::prelude::v1::test;
    use gpui_kit::component::input::FoldRange;

    /// An outer block over lines 0..=9 holding a nested block 2..=6, which holds 3..=5,
    /// followed by a sibling block 11..=14.
    fn nested_candidates() -> Vec<FoldRange> {
        vec![
            FoldRange::new(0, 9),
            FoldRange::new(2, 6),
            FoldRange::new(3, 5),
            FoldRange::new(11, 14),
        ]
    }

    #[test]
    fn test_fold_depth_counts_the_deepest_nesting() {
        assert_eq!(fold_depth(&[]), 0);
        assert_eq!(fold_depth(&[FoldRange::new(0, 4)]), 1);
        assert_eq!(fold_depth(&nested_candidates()), 3);
    }

    #[test]
    fn test_fold_depth_treats_siblings_as_the_same_level() {
        let siblings = [
            FoldRange::new(0, 3),
            FoldRange::new(3, 6),
            FoldRange::new(7, 9),
        ];
        assert_eq!(fold_depth(&siblings), 1);
    }

    #[test]
    fn test_enclosing_candidates_list_the_hiding_blocks_outermost_first() {
        let candidates = nested_candidates();

        assert_eq!(
            enclosing_candidates(&candidates, 4),
            vec![
                FoldRange::new(0, 9),
                FoldRange::new(2, 6),
                FoldRange::new(3, 5)
            ]
        );
        // A block's first and last lines stay visible when it is folded.
        assert_eq!(
            enclosing_candidates(&candidates, 2),
            vec![FoldRange::new(0, 9)]
        );
        assert_eq!(
            enclosing_candidates(&candidates, 6),
            vec![FoldRange::new(0, 9)]
        );
        assert_eq!(enclosing_candidates(&candidates, 10), Vec::new());
    }

    #[test]
    fn test_innermost_candidate_includes_the_first_and_last_lines() {
        let candidates = nested_candidates();

        assert_eq!(
            innermost_candidate_at(&candidates, 4),
            Some(FoldRange::new(3, 5))
        );
        assert_eq!(
            innermost_candidate_at(&candidates, 3),
            Some(FoldRange::new(3, 5))
        );
        assert_eq!(
            innermost_candidate_at(&candidates, 6),
            Some(FoldRange::new(2, 6))
        );
        assert_eq!(
            innermost_candidate_at(&candidates, 12),
            Some(FoldRange::new(11, 14))
        );
        assert_eq!(innermost_candidate_at(&candidates, 10), None);
    }
}
