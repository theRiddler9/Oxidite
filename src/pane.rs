//! Multi-pane terminal splitting, binary layout tree, and pane geometry.

use crate::input::{GridPoint, Selection};
use crate::parser::Parser;
use crate::pty::{PtySize, PtyWriter};
use crate::renderer::draw::{QuadVertex, build_quad_vertices};
use crate::terminal::Terminal;
use std::sync::{Arc, Mutex};

/// Minimum allowed split ratio to prevent panes collapsing completely.
pub const MIN_SPLIT_RATIO: f32 = 0.10;
/// Maximum allowed split ratio.
pub const MAX_SPLIT_RATIO: f32 = 0.90;
/// Thickness of the divider line separating adjacent split panes in physical pixels.
pub const PANE_DIVIDER_SIZE: f32 = 2.0;

/// Direction to split a terminal pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Horizontal split divider (splits top and bottom).
    Horizontal,
    /// Vertical split divider (splits left and right).
    Vertical,
}

/// Geometric bounding box and cell dimensions for a terminal pane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PaneRect {
    /// Left physical X coordinate in window space.
    pub x: f32,
    /// Top physical Y coordinate in window space.
    pub y: f32,
    /// Total pane width in pixels.
    pub width: f32,
    /// Total pane height in pixels.
    pub height: f32,
    /// Number of text character columns that fit in this pane.
    pub cols: usize,
    /// Number of text rows that fit in this pane.
    pub rows: usize,
}

impl PaneRect {
    /// Creates a new `PaneRect` with given pixel bounds and grid dimensions.
    #[must_use]
    pub fn new(x: f32, y: f32, width: f32, height: f32, cols: usize, rows: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
            cols,
            rows,
        }
    }

    /// Checks if a window pixel point `(px, py)` falls within this pane.
    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < (self.x + self.width) && py >= self.y && py < (self.y + self.height)
    }

    /// Translates a window pixel coordinate `(px, py)` to a terminal grid column and row.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn pixel_to_grid_point(
        &self,
        px: f32,
        py: f32,
        cell_w: f32,
        cell_h: f32,
        pad_x: f32,
        pad_y: f32,
    ) -> Option<GridPoint> {
        if !self.contains(px, py) || cell_w <= 0.0 || cell_h <= 0.0 {
            return None;
        }
        let rel_x = (px - self.x - pad_x).max(0.0);
        let rel_y = (py - self.y - pad_y).max(0.0);
        let col = (rel_x / cell_w) as usize;
        let row = (rel_y / cell_h) as usize;
        Some(GridPoint::new(
            col.min(self.cols.saturating_sub(1)),
            row.min(self.rows.saturating_sub(1)),
        ))
    }
}

/// A draggable separator dividing two split panes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitDivider {
    /// ID of the split node that owns this divider.
    pub split_id: usize,
    /// Split orientation.
    pub direction: SplitDirection,
    /// Left X coordinate.
    pub x: f32,
    /// Top Y coordinate.
    pub y: f32,
    /// Width in physical pixels.
    pub width: f32,
    /// Height in physical pixels.
    pub height: f32,
}

impl SplitDivider {
    /// Checks if a pixel point `(px, py)` hits this divider within `tolerance` pixels.
    #[must_use]
    pub fn hit_test(&self, px: f32, py: f32, tolerance: f32) -> bool {
        let min_x = self.x - tolerance;
        let max_x = self.x + self.width + tolerance;
        let min_y = self.y - tolerance;
        let max_y = self.y + self.height + tolerance;
        px >= min_x && px <= max_x && py >= min_y && py <= max_y
    }
}

/// A node within the binary split pane tree.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneNode {
    /// A single terminal pane leaf.
    Leaf(usize),
    /// A split dividing child nodes into two halves.
    Split {
        /// Unique identifier for this split operation.
        id: usize,
        /// Split orientation.
        direction: SplitDirection,
        /// Proportion allocated to the first child (clamped to `MIN_SPLIT_RATIO..=MAX_SPLIT_RATIO`).
        ratio: f32,
        /// First child (left or top).
        first: Box<PaneNode>,
        /// Second child (right or bottom).
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Returns a list of all pane IDs contained in this subtree in depth-first order.
    #[must_use]
    pub fn all_leaf_ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        self.collect_leaf_ids(&mut ids);
        ids
    }

    fn collect_leaf_ids(&self, ids: &mut Vec<usize>) {
        match self {
            Self::Leaf(id) => ids.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_leaf_ids(ids);
                second.collect_leaf_ids(ids);
            }
        }
    }

    /// Checks if this subtree contains the given pane ID.
    #[must_use]
    pub fn contains_leaf(&self, pane_id: usize) -> bool {
        match self {
            Self::Leaf(id) => *id == pane_id,
            Self::Split { first, second, .. } => {
                first.contains_leaf(pane_id) || second.contains_leaf(pane_id)
            }
        }
    }

    /// Splits an existing leaf pane identified by `target_id` into two siblings.
    pub fn split_leaf(
        &mut self,
        target_id: usize,
        new_id: usize,
        split_id: usize,
        direction: SplitDirection,
    ) -> bool {
        match self {
            Self::Leaf(id) if *id == target_id => {
                let first = Box::new(Self::Leaf(*id));
                let second = Box::new(Self::Leaf(new_id));
                *self = Self::Split {
                    id: split_id,
                    direction,
                    ratio: 0.5,
                    first,
                    second,
                };
                true
            }
            Self::Split { first, second, .. } => {
                if first.split_leaf(target_id, new_id, split_id, direction) {
                    true
                } else {
                    second.split_leaf(target_id, new_id, split_id, direction)
                }
            }
            Self::Leaf(_) => false,
        }
    }

    /// Removes a pane leaf from the tree, collapsing its parent split into the sibling node.
    /// Returns `None` if the root leaf itself is removed, leaving the tree empty.
    pub fn remove_leaf(self, target_id: usize) -> Option<Self> {
        match self {
            Self::Leaf(id) => {
                if id == target_id {
                    None
                } else {
                    Some(Self::Leaf(id))
                }
            }
            Self::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => {
                if first.contains_leaf(target_id) {
                    let new_first = first.remove_leaf(target_id);
                    match new_first {
                        Some(f) => Some(Self::Split {
                            id,
                            direction,
                            ratio,
                            first: Box::new(f),
                            second,
                        }),
                        None => Some(*second),
                    }
                } else if second.contains_leaf(target_id) {
                    let new_second = second.remove_leaf(target_id);
                    match new_second {
                        Some(s) => Some(Self::Split {
                            id,
                            direction,
                            ratio,
                            first,
                            second: Box::new(s),
                        }),
                        None => Some(*first),
                    }
                } else {
                    Some(Self::Split {
                        id,
                        direction,
                        ratio,
                        first,
                        second,
                    })
                }
            }
        }
    }

    /// Adjusts the split ratio of a split node identified by `split_id`.
    pub fn adjust_split_ratio(&mut self, target_split_id: usize, new_ratio: f32) -> bool {
        match self {
            Self::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                if *id == target_split_id {
                    *ratio = new_ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
                    true
                } else {
                    first.adjust_split_ratio(target_split_id, new_ratio)
                        || second.adjust_split_ratio(target_split_id, new_ratio)
                }
            }
            Self::Leaf(_) => false,
        }
    }

    /// Computes layout rectangles for all leaves and collects divider geometries.
    #[allow(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn compute_layout(
        &self,
        rect: PaneRect,
        cell_w: f32,
        cell_h: f32,
        divider_size: f32,
        pad_x: f32,
        pad_y: f32,
        layouts: &mut Vec<(usize, PaneRect)>,
        dividers: &mut Vec<SplitDivider>,
    ) {
        match self {
            Self::Leaf(id) => {
                let avail_w = (rect.width - 2.0 * pad_x).max(0.0);
                let avail_h = (rect.height - 2.0 * pad_y).max(0.0);
                let cols = if cell_w > 0.0 {
                    (avail_w / cell_w) as usize
                } else {
                    80
                };
                let rows = if cell_h > 0.0 {
                    (avail_h / cell_h) as usize
                } else {
                    24
                };
                layouts.push((
                    *id,
                    PaneRect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                        cols: cols.max(1),
                        rows: rows.max(1),
                    },
                ));
            }
            Self::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => match direction {
                SplitDirection::Vertical => {
                    let total_avail = (rect.width - divider_size).max(0.0);
                    let w1 = (total_avail * *ratio).round();
                    let w2 = (total_avail - w1).max(0.0);
                    let div_x = rect.x + w1;

                    dividers.push(SplitDivider {
                        split_id: *id,
                        direction: SplitDirection::Vertical,
                        x: div_x,
                        y: rect.y,
                        width: divider_size,
                        height: rect.height,
                    });

                    let r1 = PaneRect {
                        x: rect.x,
                        y: rect.y,
                        width: w1,
                        height: rect.height,
                        cols: 0,
                        rows: 0,
                    };
                    let r2 = PaneRect {
                        x: div_x + divider_size,
                        y: rect.y,
                        width: w2,
                        height: rect.height,
                        cols: 0,
                        rows: 0,
                    };

                    first.compute_layout(
                        r1,
                        cell_w,
                        cell_h,
                        divider_size,
                        pad_x,
                        pad_y,
                        layouts,
                        dividers,
                    );
                    second.compute_layout(
                        r2,
                        cell_w,
                        cell_h,
                        divider_size,
                        pad_x,
                        pad_y,
                        layouts,
                        dividers,
                    );
                }
                SplitDirection::Horizontal => {
                    let total_avail = (rect.height - divider_size).max(0.0);
                    let h1 = (total_avail * *ratio).round();
                    let h2 = (total_avail - h1).max(0.0);
                    let div_y = rect.y + h1;

                    dividers.push(SplitDivider {
                        split_id: *id,
                        direction: SplitDirection::Horizontal,
                        x: rect.x,
                        y: div_y,
                        width: rect.width,
                        height: divider_size,
                    });

                    let r1 = PaneRect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: h1,
                        cols: 0,
                        rows: 0,
                    };
                    let r2 = PaneRect {
                        x: rect.x,
                        y: div_y + divider_size,
                        width: rect.width,
                        height: h2,
                        cols: 0,
                        rows: 0,
                    };

                    first.compute_layout(
                        r1,
                        cell_w,
                        cell_h,
                        divider_size,
                        pad_x,
                        pad_y,
                        layouts,
                        dividers,
                    );
                    second.compute_layout(
                        r2,
                        cell_w,
                        cell_h,
                        divider_size,
                        pad_x,
                        pad_y,
                        layouts,
                        dividers,
                    );
                }
            },
        }
    }
}

/// A binary tree managing split panes within a tab.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneTree {
    /// Root node of the pane split tree.
    pub root: PaneNode,
    /// Counter for assigning unique IDs to split operations.
    pub next_split_id: usize,
}

impl PaneTree {
    /// Creates a new `PaneTree` with a single initial root pane.
    #[must_use]
    pub fn new(root_pane_id: usize) -> Self {
        Self {
            root: PaneNode::Leaf(root_pane_id),
            next_split_id: 1,
        }
    }

    /// Splits the pane `target_id` into two using `new_id`. Returns `true` on success.
    pub fn split(&mut self, target_id: usize, new_id: usize, direction: SplitDirection) -> bool {
        let split_id = self.next_split_id;
        self.next_split_id += 1;
        self.root.split_leaf(target_id, new_id, split_id, direction)
    }

    /// Closes a pane identified by `pane_id`. Returns `false` if the last pane was closed.
    pub fn close_pane(&mut self, pane_id: usize) -> bool {
        let root = std::mem::replace(&mut self.root, PaneNode::Leaf(0));
        match root.remove_leaf(pane_id) {
            Some(new_root) => {
                self.root = new_root;
                true
            }
            None => false,
        }
    }

    /// Returns the number of visible panes.
    #[must_use]
    pub fn panes_count(&self) -> usize {
        self.root.all_leaf_ids().len()
    }

    /// Returns all pane IDs in DFS order.
    #[must_use]
    pub fn all_panes(&self) -> Vec<usize> {
        self.root.all_leaf_ids()
    }

    /// Returns the next pane ID in circular order from `current_id`.
    #[must_use]
    pub fn next_pane(&self, current_id: usize) -> usize {
        let all = self.all_panes();
        if all.is_empty() {
            return current_id;
        }
        if let Some(pos) = all.iter().position(|&id| id == current_id) {
            all[(pos + 1) % all.len()]
        } else {
            all[0]
        }
    }

    /// Returns the previous pane ID in circular order from `current_id`.
    #[must_use]
    pub fn prev_pane(&self, current_id: usize) -> usize {
        let all = self.all_panes();
        if all.is_empty() {
            return current_id;
        }
        if let Some(pos) = all.iter().position(|&id| id == current_id) {
            all[(pos + all.len() - 1) % all.len()]
        } else {
            all[0]
        }
    }

    /// Computes layout rectangles for all panes and split dividers within `container`.
    #[must_use]
    pub fn compute_layout(
        &self,
        container: PaneRect,
        cell_w: f32,
        cell_h: f32,
        pad_x: f32,
        pad_y: f32,
    ) -> (Vec<(usize, PaneRect)>, Vec<SplitDivider>) {
        let mut layouts = Vec::new();
        let mut dividers = Vec::new();
        self.root.compute_layout(
            container,
            cell_w,
            cell_h,
            PANE_DIVIDER_SIZE,
            pad_x,
            pad_y,
            &mut layouts,
            &mut dividers,
        );
        (layouts, dividers)
    }

    /// Finds which pane leaf contains the window coordinate `(px, py)`.
    #[must_use]
    pub fn find_pane_at(layouts: &[(usize, PaneRect)], px: f32, py: f32) -> Option<usize> {
        layouts
            .iter()
            .find(|(_, rect)| rect.contains(px, py))
            .map(|(id, _)| *id)
    }

    /// Tests if `(px, py)` hits any split divider within `tolerance` pixels.
    #[must_use]
    pub fn hit_test_divider(
        dividers: &[SplitDivider],
        px: f32,
        py: f32,
        tolerance: f32,
    ) -> Option<SplitDivider> {
        dividers
            .iter()
            .copied()
            .find(|d| d.hit_test(px, py, tolerance))
    }

    /// Builds quads for drawing split divider separator lines.
    #[must_use]
    pub fn build_divider_quads(dividers: &[SplitDivider], color: [f32; 4]) -> Vec<QuadVertex> {
        let mut vertices = Vec::new();
        for div in dividers {
            vertices.extend_from_slice(&build_quad_vertices(
                div.x, div.y, div.width, div.height, color,
            ));
        }
        vertices
    }

    /// Builds quads for drawing a 1.5px focus border around the active pane.
    #[must_use]
    pub fn build_pane_border_quads(
        rect: &PaneRect,
        color: [f32; 4],
        thickness: f32,
    ) -> Vec<QuadVertex> {
        let mut vertices = Vec::new();
        let t = thickness.max(1.0);
        // Top edge
        vertices.extend_from_slice(&build_quad_vertices(rect.x, rect.y, rect.width, t, color));
        // Bottom edge
        vertices.extend_from_slice(&build_quad_vertices(
            rect.x,
            rect.y + rect.height - t,
            rect.width,
            t,
            color,
        ));
        // Left edge
        vertices.extend_from_slice(&build_quad_vertices(rect.x, rect.y, t, rect.height, color));
        // Right edge
        vertices.extend_from_slice(&build_quad_vertices(
            rect.x + rect.width - t,
            rect.y,
            t,
            rect.height,
            color,
        ));
        vertices
    }
}

/// An individual terminal pane with its own PTY, Parser, Terminal state, and text selection.
pub struct Pane {
    /// Unique identifier for this pane.
    pub id: usize,
    /// Title displayed for this pane.
    pub title: String,
    /// Terminal state containing grid, cursor, modes, and scrollback.
    pub terminal: Terminal,
    /// VT escape sequence parser.
    pub parser: Parser,
    /// PTY master writer for forwarding keyboard/mouse input and resize signals.
    pub pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    /// Whether the underlying shell process has exited.
    pub pty_exited: bool,
    /// Child process exit code if terminated.
    pub pty_exit_code: Option<i32>,
    /// Text selection range within this pane's grid.
    pub selection: Option<Selection>,
    /// Mouse drag start coordinate when selecting text.
    pub mouse_down_pos: Option<GridPoint>,
    /// Detected current working directory for this pane.
    pub cwd: Option<std::path::PathBuf>,
    /// Current geometric rectangle and grid dimensions of this pane.
    pub rect: PaneRect,
}

impl Pane {
    /// Creates a new `Pane` instance.
    #[must_use]
    pub fn new(
        id: usize,
        title: String,
        terminal: Terminal,
        pty_writer: Option<Arc<Mutex<PtyWriter>>>,
    ) -> Self {
        Self {
            id,
            title,
            terminal,
            parser: Parser::new(),
            pty_writer,
            pty_exited: false,
            pty_exit_code: None,
            selection: None,
            mouse_down_pos: None,
            cwd: None,
            rect: PaneRect::default(),
        }
    }

    /// Feeds incoming PTY bytes into this pane's VT parser and updates terminal state.
    pub fn advance_parser(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
        if !self.terminal.title.is_empty() {
            self.title = self.terminal.title.clone();
        }
        if let Some(ref cwd) = self.terminal.cwd {
            self.cwd = Some(cwd.clone());
        }
    }

    /// Resizes the pane grid and sends resize notification to the PTY.
    pub fn resize(&mut self, cols: usize, rows: usize, pty_size: PtySize) {
        self.terminal.resize(cols, rows);
        if let Some(ref writer) = self.pty_writer
            && let Ok(mut guard) = writer.lock()
        {
            let _ = guard.resize(pty_size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pane_tree() {
        let tree = PaneTree::new(10);
        assert_eq!(tree.panes_count(), 1);
        assert_eq!(tree.all_panes(), vec![10]);
        assert_eq!(tree.next_pane(10), 10);
        assert_eq!(tree.prev_pane(10), 10);

        let container = PaneRect::new(0.0, 0.0, 800.0, 600.0, 80, 24);
        let (layouts, dividers) = tree.compute_layout(container, 10.0, 20.0, 0.0, 0.0);
        assert_eq!(layouts.len(), 1);
        assert_eq!(layouts[0].0, 10);
        assert_eq!(layouts[0].1.width, 800.0);
        assert_eq!(layouts[0].1.height, 600.0);
        assert_eq!(layouts[0].1.cols, 80);
        assert_eq!(layouts[0].1.rows, 30);
        assert!(dividers.is_empty());
    }

    #[test]
    fn test_vertical_and_horizontal_split() {
        let mut tree = PaneTree::new(1);
        assert!(tree.split(1, 2, SplitDirection::Vertical));
        assert_eq!(tree.panes_count(), 2);
        assert_eq!(tree.all_panes(), vec![1, 2]);

        let container = PaneRect::new(0.0, 0.0, 802.0, 600.0, 80, 24);
        let (layouts, dividers) = tree.compute_layout(container, 10.0, 20.0, 0.0, 0.0);
        assert_eq!(layouts.len(), 2);
        assert_eq!(dividers.len(), 1);
        assert_eq!(layouts[0].0, 1);
        assert_eq!(layouts[0].1.width, 400.0);
        assert_eq!(layouts[1].0, 2);
        assert_eq!(layouts[1].1.width, 400.0);
        assert_eq!(dividers[0].x, 400.0);
        assert_eq!(dividers[0].direction, SplitDirection::Vertical);

        // Split pane 2 horizontally
        assert!(tree.split(2, 3, SplitDirection::Horizontal));
        assert_eq!(tree.panes_count(), 3);
        assert_eq!(tree.all_panes(), vec![1, 2, 3]);

        let (layouts, dividers) = tree.compute_layout(container, 10.0, 20.0, 0.0, 0.0);
        assert_eq!(layouts.len(), 3);
        assert_eq!(dividers.len(), 2);

        // Circular navigation
        assert_eq!(tree.next_pane(1), 2);
        assert_eq!(tree.next_pane(2), 3);
        assert_eq!(tree.next_pane(3), 1);
        assert_eq!(tree.prev_pane(1), 3);
    }

    #[test]
    fn test_close_pane_and_collapse_split() {
        let mut tree = PaneTree::new(1);
        tree.split(1, 2, SplitDirection::Vertical);
        tree.split(2, 3, SplitDirection::Horizontal);
        assert_eq!(tree.panes_count(), 3);

        // Close pane 2
        assert!(tree.close_pane(2));
        assert_eq!(tree.panes_count(), 2);
        assert_eq!(tree.all_panes(), vec![1, 3]);

        // Close pane 3 -> leaves only pane 1
        assert!(tree.close_pane(3));
        assert_eq!(tree.panes_count(), 1);
        assert_eq!(tree.all_panes(), vec![1]);

        // Close last pane -> returns false
        assert!(!tree.close_pane(1));
    }

    #[test]
    fn test_divider_hit_test_and_quad_builders() {
        let div = SplitDivider {
            split_id: 1,
            direction: SplitDirection::Vertical,
            x: 100.0,
            y: 50.0,
            width: 2.0,
            height: 200.0,
        };
        assert!(div.hit_test(101.0, 100.0, 2.0));
        assert!(div.hit_test(99.0, 100.0, 2.0));
        assert!(!div.hit_test(110.0, 100.0, 2.0));

        let quads = PaneTree::build_divider_quads(&[div], [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(quads.len(), 6);

        let border_quads = PaneTree::build_pane_border_quads(
            &PaneRect::new(10.0, 10.0, 100.0, 50.0, 10, 5),
            [0.3, 0.6, 1.0, 1.0],
            2.0,
        );
        // 4 edges * 6 vertices = 24
        assert_eq!(border_quads.len(), 24);
    }
}
