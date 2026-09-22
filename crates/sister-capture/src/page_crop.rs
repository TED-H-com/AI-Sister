//! Which on-screen PDF page, if any, this walk may clip to.
//!
//! The Windows walker only records what it actually observed. This module
//! decides. It is pure so a Linux `cargo test` can run it: the COM walk in
//! `windows/text.rs` is `#[cfg(windows)]` and Windows CI does not execute it.
//!
//! A finished walk clips when the monitor was measured and exactly one
//! observed node qualifies. An unfinished walk clips only when that monitor
//! was measured, every depth-1 direct child was observed, and exactly one of
//! those depth-1 nodes qualifies. A qualifying node deeper than that does not
//! stand in for the page while nodes are still uncounted. A failed sibling
//! read is not the end of that list: the walk is unfinished, and a failed
//! read of the depth-1 list leaves that layer incomplete even when every
//! element that came back was observed.
use std::collections::VecDeque;

use crate::assistive::TextRect;

/// Nodes visited before the walk refuses to start another one.
///
/// The cap is the bound on work, not a page count. Stopping because this many
/// nodes were already visited means nodes are still uncounted.
pub(crate) const PAGE_WALK_NODE_CAP: u32 = 128;

/// Deepest visited depth. Direct children of the focused Document are depth 1.
///
/// A node at this depth is visited. Its children are not. A child that exists
/// there, or a child probe that was never made, means the walk did not finish.
pub(crate) const PAGE_WALK_DEPTH_CAP: u32 = 6;

/// Direct children of the focused Document. Not a measured node count.
const DIRECT_CHILD_DEPTH: u32 = 1;

/// Width and height below which a Group is a thumbnail or a sidebar.
///
/// This is not the page's real size. A zoomed-out page can fall under it and
/// will not qualify; a wide sidebar can clear it and will.
pub(crate) const PAGE_GROUP_MIN_WIDTH: f64 = 200.0;
pub(crate) const PAGE_GROUP_MIN_HEIGHT: f64 = 80.0;

/// One node the walk looked at. `None` is "this read did not return", which
/// is not a measured false and not a measured 0×0 rectangle.
#[derive(Clone, Copy)]
pub(crate) struct PageNode {
    pub is_group: Option<bool>,
    pub offscreen: Option<bool>,
    pub bounds: Option<TextRect>,
    /// Distance from the focused Document. Direct children are depth 1.
    pub depth: u32,
}

/// `Crop` is the only result that may clip. The other two both leave the
/// document ranges alone, and they are not the same result: a finished walk
/// that found no page is not an unfinished count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageCrop {
    /// The index is into the candidate slice the caller passed. The monitor
    /// was measured. Either the walk finished and exactly one observed node
    /// qualified, or the walk did not finish, depth 1 was fully observed, and
    /// exactly one depth-1 node qualified.
    Crop(usize),
    /// Walk finished, the monitor was measured, and no node qualified.
    NoQualifiedPage,
    /// The monitor was not measured, more than one candidate qualified, or
    /// the unfinished walk did not have exactly one fully observed depth-1
    /// page. Do not clip.
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reject {
    NotAGroup,
    ControlTypeUnmeasured,
    Offscreen,
    OffscreenUnmeasured,
    BoundsUnmeasured,
    BelowSizeFloor,
    OutsideMonitor,
}

/// What to do with the node we are about to visit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frontier {
    /// Visit this node. `descend` is false only at the depth cap when the
    /// child probe measured no child.
    Open { descend: bool },
    /// `visited` already reached [`PAGE_WALK_NODE_CAP`]. This node is not
    /// counted, and neither is anything still queued.
    BudgetLeftNodes,
    /// This node is at the depth cap and either has a child or the caller
    /// did not probe. `None` is not "measured no child".
    DepthLeftChildren,
}

/// `visited` is how many nodes have already been counted, before this one.
///
/// `child_at_cap` is consulted only at the depth cap. Below the cap, children
/// are read through [`sibling_chain`], which keeps a failed sibling read apart
/// from the end of the list. At the cap, `Some(false)` is the measured leaf,
/// `Some(true)` means a child exists, and `None` means the probe did not
/// return a measurement.
pub(crate) fn frontier(visited: u32, depth: u32, child_at_cap: Option<bool>) -> Frontier {
    if visited >= PAGE_WALK_NODE_CAP {
        return Frontier::BudgetLeftNodes;
    }
    if depth < PAGE_WALK_DEPTH_CAP {
        return Frontier::Open { descend: true };
    }
    match child_at_cap {
        Some(false) => Frontier::Open { descend: false },
        Some(true) | None => Frontier::DepthLeftChildren,
    }
}

/// One sibling read. The end of the list and a failed read are different
/// answers. Callers must not collapse them into one `Option`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SiblingRead<T> {
    /// This sibling was read.
    Item(T),
    /// The list ended. Nothing was returned.
    End,
    /// The read failed. Siblings already collected are a prefix.
    Failed,
}

/// Siblings in list order. `truncated_by_error` means `items` is a prefix,
/// not the whole layer.
pub(crate) struct SiblingChain<T> {
    pub(crate) items: Vec<T>,
    /// A sibling read failed. `items` is a prefix, not the whole layer.
    pub(crate) truncated_by_error: bool,
}

impl<T> SiblingChain<T> {
    /// The whole layer was read. Tests and a walk that really finished share this.
    pub(crate) fn complete(items: impl IntoIterator<Item = T>) -> Self {
        Self {
            items: items.into_iter().collect(),
            truncated_by_error: false,
        }
    }

    /// A read failed. `items` is only the prefix that came back.
    pub(crate) fn truncated(items: impl IntoIterator<Item = T>) -> Self {
        Self {
            items: items.into_iter().collect(),
            truncated_by_error: true,
        }
    }
}

/// Every sibling from `first`, inclusive, in sibling order.
///
/// `End` stops the chain. `Failed` stops it too and marks the items already
/// collected as a prefix. Those two results are not interchangeable.
pub(crate) fn sibling_chain<T>(
    mut first: SiblingRead<T>,
    mut next: impl FnMut(&T) -> SiblingRead<T>,
) -> SiblingChain<T> {
    let mut items = Vec::new();
    loop {
        match first {
            SiblingRead::End => return SiblingChain::complete(items),
            SiblingRead::Failed => return SiblingChain::truncated(items),
            SiblingRead::Item(current) => {
                let following = next(&current);
                items.push(current);
                first = following;
            }
        }
    }
}

/// Depth of one queued node. Only the walk constructs these: direct children
/// start at depth 1, and each enqueued child is one deeper than its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalkDepth(u32);

impl WalkDepth {
    pub(crate) fn get(self) -> u32 {
        self.0
    }

    fn is_direct_child(self) -> bool {
        self.0 == DIRECT_CHILD_DEPTH
    }

    fn child(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Whether every direct child was enqueued and then observed.
///
/// `false` is a measured incomplete layer: the observed count is short of the
/// enqueued direct children, the node cap refused a direct child, or the
/// depth-1 sibling read failed and the enqueued nodes are only a prefix.
/// The cap and the failed read are separate arguments. A failed read can
/// leave the two counts equal; that is not a fully seen layer. `false` is
/// not "the layer was never checked".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DepthOneComplete {
    complete: bool,
}

impl DepthOneComplete {
    #[must_use]
    fn from_walk(
        direct_enqueued: usize,
        depth_one_observed: usize,
        direct_child_blocked_by_node_cap: bool,
        direct_child_truncated_by_error: bool,
    ) -> Self {
        Self {
            complete: direct_enqueued == depth_one_observed
                && !direct_child_blocked_by_node_cap
                && !direct_child_truncated_by_error,
        }
    }

    fn is_complete(self) -> bool {
        self.complete
    }
}

/// Breadth-first queue of direct children, then whatever those nodes contain.
///
/// Seeding pushes every direct child before the walk pops any of them.
/// Children of a visited node are appended, so the rest of depth 1 is popped
/// before those children. `text.rs` keeps the COM element in this queue; the
/// candidate vecs it builds stay in pop order.
pub(crate) struct BreadthWalk<T> {
    pending: VecDeque<(T, WalkDepth)>,
    visited: u32,
    /// Cleared when a cap refuses a queued node. Those nodes stay unobserved.
    /// A failed sibling read does not clear this: the nodes already read are
    /// still visited. Stopping here as well would make the prefix look short,
    /// and the short count would hide a dropped failure flag.
    finished: bool,
    /// Some sibling read failed, at any depth. The visited nodes are a prefix
    /// of the tree. Reported as an unfinished walk without dropping the queue.
    sibling_chain_truncated: bool,
    /// How many direct children were actually enqueued. A failed depth-1 read
    /// leaves this equal to the prefix length. It does not count the sibling
    /// that failed to come back; that failure is the flag below.
    direct_enqueued: usize,
    depth_one_observed: usize,
    direct_child_blocked_by_node_cap: bool,
    /// The depth-1 sibling read failed. Separate from the node-cap flag:
    /// the two counts can still match.
    direct_child_truncated_by_error: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WalkOutcome {
    pub walk_finished: bool,
    pub depth_one: DepthOneComplete,
}

impl<T> BreadthWalk<T> {
    pub(crate) fn seed(direct_children: SiblingChain<T>) -> Self {
        let direct_child_truncated_by_error = direct_children.truncated_by_error;
        let pending: VecDeque<(T, WalkDepth)> = direct_children
            .items
            .into_iter()
            .map(|child| (child, WalkDepth(DIRECT_CHILD_DEPTH)))
            .collect();
        let direct_enqueued = pending.len();
        Self {
            pending,
            visited: 0,
            finished: true,
            sibling_chain_truncated: direct_child_truncated_by_error,
            direct_enqueued,
            depth_one_observed: 0,
            direct_child_blocked_by_node_cap: false,
            direct_child_truncated_by_error,
        }
    }

    /// How many nodes have been recorded. The frontier sees this before the
    /// node in hand is recorded.
    pub(crate) fn visited(&self) -> u32 {
        self.visited
    }

    /// Next node, or `None` when the queue is empty or the walk has stopped.
    /// A stop leaves the remaining queued nodes unobserved.
    #[must_use]
    pub(crate) fn pop_front(&mut self) -> Option<(T, WalkDepth)> {
        if !self.finished {
            return None;
        }
        self.pending.pop_front()
    }

    /// Queue `children` behind everything already pending, one level under
    /// `parent`. Pass the depth `pop_front` returned for that parent.
    ///
    /// A failed read marks the walk unfinished and still queues the prefix.
    /// Children of a visited node are deeper than depth 1, so this does not
    /// clear the depth-1 layer.
    pub(crate) fn enqueue_children(&mut self, parent: WalkDepth, children: SiblingChain<T>) {
        if children.truncated_by_error {
            self.sibling_chain_truncated = true;
        }
        let child_depth = parent.child();
        self.pending
            .extend(children.items.into_iter().map(|child| (child, child_depth)));
    }

    /// Count this node. The returned depth is the one to store on [`PageNode`].
    #[must_use]
    pub(crate) fn record_observed(&mut self, depth: WalkDepth) -> u32 {
        self.visited += 1;
        if depth.is_direct_child() {
            self.depth_one_observed += 1;
        }
        depth.get()
    }

    /// The node cap refused the node just popped. It is not recorded. When
    /// that node is a direct child, depth 1 was not fully seen; a deeper
    /// refusal leaves an already-finished depth-1 count alone.
    pub(crate) fn stop_for_node_cap(&mut self, depth: WalkDepth) {
        self.finished = false;
        if depth.is_direct_child() {
            self.direct_child_blocked_by_node_cap = true;
        }
    }

    /// This node was recorded, and it still has children past the depth cap,
    /// or the caller did not probe. Depth 1 is already behind us.
    pub(crate) fn stop_for_depth_cap(&mut self) {
        self.finished = false;
    }

    #[must_use]
    pub(crate) fn outcome(&self) -> WalkOutcome {
        WalkOutcome {
            walk_finished: self.finished && !self.sibling_chain_truncated,
            depth_one: DepthOneComplete::from_walk(
                self.direct_enqueued,
                self.depth_one_observed,
                self.direct_child_blocked_by_node_cap,
                self.direct_child_truncated_by_error,
            ),
        }
    }
}

fn rejection(node: &PageNode, monitor: TextRect) -> Option<Reject> {
    match node.is_group {
        Some(true) => {}
        Some(false) => return Some(Reject::NotAGroup),
        None => return Some(Reject::ControlTypeUnmeasured),
    }
    match node.offscreen {
        Some(false) => {}
        Some(true) => return Some(Reject::Offscreen),
        None => return Some(Reject::OffscreenUnmeasured),
    }
    let Some(bounds) = node.bounds else {
        return Some(Reject::BoundsUnmeasured);
    };
    // 0×0 is a measured rectangle that fails this floor. It is not
    // `BoundsUnmeasured`: that arm is only the missing read above.
    if !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width < PAGE_GROUP_MIN_WIDTH
        || bounds.height < PAGE_GROUP_MIN_HEIGHT
    {
        return Some(Reject::BelowSizeFloor);
    }
    if !bounds.overlaps(monitor) {
        return Some(Reject::OutsideMonitor);
    }
    None
}

#[derive(Clone, Copy)]
enum Candidates {
    /// Unfinished walk whose direct children were all observed.
    DepthOne,
    /// Finished walk. Every observed node counts, at any depth.
    AllObserved,
}

fn select(nodes: &[PageNode], monitor: TextRect, which: Candidates) -> PageCrop {
    let mut chosen = None;
    for (index, node) in nodes.iter().enumerate() {
        if matches!(which, Candidates::DepthOne) && node.depth != DIRECT_CHILD_DEPTH {
            continue;
        }
        if rejection(node, monitor).is_some() {
            continue;
        }
        if chosen.is_some() {
            return PageCrop::Unresolved;
        }
        chosen = Some(index);
    }
    match (chosen, which) {
        (Some(index), _) => PageCrop::Crop(index),
        (None, Candidates::DepthOne) => PageCrop::Unresolved,
        (None, Candidates::AllObserved) => PageCrop::NoQualifiedPage,
    }
}

pub(crate) fn choose_page_crop(
    nodes: &[PageNode],
    monitor: Option<TextRect>,
    walk_finished: bool,
    depth_one: DepthOneComplete,
) -> PageCrop {
    // No monitor rectangle means the overlap test did not run. That is not
    // "we measured a screen and nothing was on it", and it is not a depth-1
    // page either. This is checked before the unfinished-walk exception.
    let Some(monitor) = monitor else {
        return PageCrop::Unresolved;
    };
    if walk_finished {
        return select(nodes, monitor, Candidates::AllObserved);
    }
    if depth_one.is_complete() {
        return select(nodes, monitor, Candidates::DepthOne);
    }
    PageCrop::Unresolved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor() -> TextRect {
        TextRect {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        }
    }

    fn page_at(x: f64, y: f64, width: f64, height: f64) -> PageNode {
        PageNode {
            is_group: Some(true),
            offscreen: Some(false),
            bounds: Some(TextRect {
                x,
                y,
                width,
                height,
            }),
            depth: 1,
        }
    }

    fn good() -> PageNode {
        page_at(100.0, 80.0, 800.0, 600.0)
    }

    /// Two direct children were enqueued, one was observed, and the node cap
    /// refused the other. Old calls pass this so an unfinished walk stays
    /// unresolved; a finished walk does not read it.
    fn depth_one_left_unobserved() -> DepthOneComplete {
        DepthOneComplete::from_walk(2, 1, true, false)
    }

    fn seen_direct(count: usize) -> DepthOneComplete {
        DepthOneComplete::from_walk(count, count, false, false)
    }

    #[test]
    fn direct_children_include_every_sibling_not_only_the_first() {
        let children = sibling_chain(SiblingRead::Item(1_u32), |n| {
            if *n < 3 {
                SiblingRead::Item(n + 1)
            } else {
                SiblingRead::End
            }
        });
        assert_eq!(children.items, vec![1, 2, 3]);
        assert!(
            sibling_chain(SiblingRead::End::<u32>, |_| SiblingRead::Item(1_u32))
                .items
                .is_empty()
        );
    }

    #[test]
    fn the_node_budget_and_a_child_past_the_depth_cap_do_not_finish_the_walk() {
        assert_eq!(
            frontier(PAGE_WALK_NODE_CAP, 1, None),
            Frontier::BudgetLeftNodes
        );
        assert_eq!(
            frontier(PAGE_WALK_NODE_CAP - 1, 1, None),
            Frontier::Open { descend: true }
        );
        assert_eq!(
            frontier(0, PAGE_WALK_DEPTH_CAP, Some(true)),
            Frontier::DepthLeftChildren
        );
        assert_eq!(
            frontier(0, PAGE_WALK_DEPTH_CAP, Some(false)),
            Frontier::Open { descend: false }
        );
        // Not probing is not a measured leaf.
        assert_eq!(
            frontier(0, PAGE_WALK_DEPTH_CAP, None),
            Frontier::DepthLeftChildren
        );
        assert_ne!(
            frontier(0, PAGE_WALK_DEPTH_CAP, None),
            frontier(0, PAGE_WALK_DEPTH_CAP, Some(false))
        );
        assert_eq!(
            frontier(0, PAGE_WALK_DEPTH_CAP - 1, None),
            Frontier::Open { descend: true }
        );
    }

    #[test]
    fn two_qualified_pages_are_not_clipped() {
        let decision = choose_page_crop(
            &[good(), good()],
            Some(monitor()),
            true,
            depth_one_left_unobserved(),
        );
        assert_eq!(decision, PageCrop::Unresolved);
        assert_ne!(decision, PageCrop::NoQualifiedPage);
    }

    #[test]
    fn one_qualified_page_is_not_enough_while_depth_one_is_unobserved() {
        let nodes = [good()];
        let unfinished =
            choose_page_crop(&nodes, Some(monitor()), false, depth_one_left_unobserved());
        // The same page with the flag flipped the other way. If the line
        // above passes only because the page itself does not qualify, this
        // one fails. Both calls leave depth 1 unobserved: with that layer
        // seen, one qualified page is enough and the first line would clip.
        let finished = choose_page_crop(&nodes, Some(monitor()), true, depth_one_left_unobserved());
        assert_eq!(unfinished, PageCrop::Unresolved);
        assert_eq!(finished, PageCrop::Crop(0));
    }

    #[test]
    fn a_finished_walk_clips_the_one_qualified_page() {
        let filler = PageNode {
            is_group: Some(false),
            offscreen: Some(false),
            bounds: Some(TextRect {
                x: 0.0,
                y: 0.0,
                width: 900.0,
                height: 700.0,
            }),
            depth: 1,
        };
        assert_eq!(
            choose_page_crop(
                &[filler, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(1)
        );
        // The floor is the constant, not "anything with a positive area".
        assert_eq!(
            choose_page_crop(
                &[page_at(0.0, 0.0, 200.0, 80.0)],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(0)
        );
    }

    #[test]
    fn a_finished_walk_with_no_qualified_page_is_not_an_unfinished_walk() {
        let offscreen = PageNode {
            offscreen: Some(true),
            ..good()
        };
        let finished_none = choose_page_crop(
            &[offscreen],
            Some(monitor()),
            true,
            depth_one_left_unobserved(),
        );
        let unfinished_none = choose_page_crop(
            &[offscreen],
            Some(monitor()),
            false,
            depth_one_left_unobserved(),
        );
        assert_eq!(finished_none, PageCrop::NoQualifiedPage);
        assert_eq!(unfinished_none, PageCrop::Unresolved);
        assert_ne!(finished_none, unfinished_none);
    }

    #[test]
    fn an_offscreen_group_is_not_a_qualified_page() {
        let offscreen = PageNode {
            offscreen: Some(true),
            ..good()
        };
        assert_eq!(rejection(&offscreen, monitor()), Some(Reject::Offscreen));
        assert_eq!(
            choose_page_crop(
                &[offscreen, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(1)
        );
        let unread = PageNode {
            offscreen: None,
            ..good()
        };
        assert_eq!(
            rejection(&unread, monitor()),
            Some(Reject::OffscreenUnmeasured)
        );
        assert_ne!(
            rejection(&offscreen, monitor()),
            rejection(&unread, monitor())
        );
        assert_eq!(
            choose_page_crop(
                &[unread],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::NoQualifiedPage
        );
    }

    #[test]
    fn a_group_under_the_size_floor_is_not_a_qualified_page() {
        let narrow = page_at(0.0, 0.0, PAGE_GROUP_MIN_WIDTH - 1.0, 600.0);
        let short = page_at(0.0, 0.0, 800.0, PAGE_GROUP_MIN_HEIGHT - 1.0);
        assert_eq!(rejection(&narrow, monitor()), Some(Reject::BelowSizeFloor));
        assert_eq!(rejection(&short, monitor()), Some(Reject::BelowSizeFloor));
        assert_eq!(
            choose_page_crop(
                &[narrow, short, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(2)
        );
    }

    #[test]
    fn a_missing_bounding_rect_is_not_a_measured_zero_rect() {
        let missing = PageNode {
            bounds: None,
            ..good()
        };
        let zero = page_at(10.0, 10.0, 0.0, 0.0);
        assert_eq!(
            rejection(&missing, monitor()),
            Some(Reject::BoundsUnmeasured)
        );
        assert_eq!(rejection(&zero, monitor()), Some(Reject::BelowSizeFloor));
        assert_ne!(rejection(&missing, monitor()), rejection(&zero, monitor()));
        assert_eq!(
            choose_page_crop(
                &[missing, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(1)
        );
        assert_eq!(
            choose_page_crop(
                &[zero, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(1)
        );
        assert_eq!(
            choose_page_crop(
                &[missing],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::NoQualifiedPage
        );
        assert_eq!(
            choose_page_crop(&[zero], Some(monitor()), true, depth_one_left_unobserved(),),
            PageCrop::NoQualifiedPage
        );
    }

    #[test]
    fn a_page_must_overlap_the_measured_monitor() {
        let elsewhere = page_at(2_000.0, 0.0, 800.0, 600.0);
        assert_eq!(
            rejection(&elsewhere, monitor()),
            Some(Reject::OutsideMonitor)
        );
        assert_eq!(
            choose_page_crop(
                &[elsewhere, good()],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(1)
        );
        // Hangs off the left edge and still meets the monitor. `overlaps`,
        // not "fully inside".
        let partial = page_at(-50.0, 0.0, 200.0, 80.0);
        assert_eq!(rejection(&partial, monitor()), None);
        assert_eq!(
            choose_page_crop(
                &[partial],
                Some(monitor()),
                true,
                depth_one_left_unobserved(),
            ),
            PageCrop::Crop(0)
        );
    }

    #[test]
    fn an_unmeasured_monitor_is_not_a_measured_screen_nothing_overlaps() {
        let unmeasured = choose_page_crop(&[good()], None, true, depth_one_left_unobserved());
        let measured_empty = choose_page_crop(
            &[good()],
            Some(TextRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }),
            true,
            depth_one_left_unobserved(),
        );
        assert_eq!(unmeasured, PageCrop::Unresolved);
        assert_eq!(measured_empty, PageCrop::NoQualifiedPage);
        assert_ne!(unmeasured, measured_empty);
    }

    #[test]
    fn an_unfinished_walk_crops_the_one_fully_seen_depth_one_page() {
        let filler = PageNode {
            is_group: Some(false),
            ..good()
        };
        // Same rectangle as a page, one level down. It qualifies, and it must
        // not veto or replace the depth-1 page while the walk is unfinished.
        let deep = PageNode { depth: 2, ..good() };
        let nodes = [filler, good(), deep];
        let decision = choose_page_crop(&nodes, Some(monitor()), false, seen_direct(2));
        assert_eq!(decision, PageCrop::Crop(1));
        assert_eq!(nodes[1].depth, 1);
        assert_eq!(nodes[2].depth, 2);
    }

    #[test]
    fn an_unfinished_walk_does_not_crop_when_a_direct_child_was_blocked() {
        let filler = PageNode {
            is_group: Some(false),
            ..good()
        };
        let nodes = [filler, good()];
        // The slice is the two nodes that were observed. A third direct child
        // was enqueued and the cap refused it, so it is not in the slice.
        let blocked = choose_page_crop(
            &nodes,
            Some(monitor()),
            false,
            DepthOneComplete::from_walk(3, 2, true, false),
        );
        // Counts match the observed pair, and the cap still refused a direct child.
        let cap_flag = choose_page_crop(
            &nodes,
            Some(monitor()),
            false,
            DepthOneComplete::from_walk(2, 2, true, false),
        );
        // The cap flag was left clear, but one enqueued direct child was never observed.
        let short_count = choose_page_crop(
            &nodes,
            Some(monitor()),
            false,
            DepthOneComplete::from_walk(3, 2, false, false),
        );
        let fully_seen = choose_page_crop(&nodes, Some(monitor()), false, seen_direct(2));
        assert_eq!(blocked, PageCrop::Unresolved);
        assert_eq!(cap_flag, PageCrop::Unresolved);
        assert_eq!(short_count, PageCrop::Unresolved);
        assert_eq!(fully_seen, PageCrop::Crop(1));
    }

    #[test]
    fn two_depth_one_pages_are_not_clipped_even_when_that_layer_was_fully_seen() {
        let nodes = [good(), good()];
        let layer = seen_direct(2);
        let two_unfinished = choose_page_crop(&nodes, Some(monitor()), false, layer);
        let two_finished = choose_page_crop(&nodes, Some(monitor()), true, layer);
        let one_unfinished = choose_page_crop(&nodes[..1], Some(monitor()), false, seen_direct(1));
        assert_eq!(two_unfinished, PageCrop::Unresolved);
        assert_eq!(two_finished, PageCrop::Unresolved);
        assert_eq!(one_unfinished, PageCrop::Crop(0));
        assert_ne!(two_unfinished, PageCrop::NoQualifiedPage);
    }

    #[test]
    fn an_unfinished_walk_does_not_crop_a_deeper_page_when_depth_one_had_none() {
        let shallow = page_at(0.0, 0.0, PAGE_GROUP_MIN_WIDTH - 1.0, 600.0);
        let deep = PageNode { depth: 2, ..good() };
        let nodes = [shallow, deep];
        let layer = seen_direct(1);
        let unfinished = choose_page_crop(&nodes, Some(monitor()), false, layer);
        let finished = choose_page_crop(&nodes, Some(monitor()), true, layer);
        assert_eq!(unfinished, PageCrop::Unresolved);
        assert_eq!(finished, PageCrop::Crop(1));
        assert_eq!(nodes[1].depth, 2);
    }

    #[test]
    fn a_finished_walk_clips_the_one_deeper_page_when_depth_one_had_none() {
        let shallow = page_at(0.0, 0.0, 10.0, 10.0);
        let deep = PageNode { depth: 2, ..good() };
        let nodes = [shallow, deep];
        assert_eq!(
            choose_page_crop(&nodes, Some(monitor()), true, seen_direct(1)),
            PageCrop::Crop(1)
        );
        assert_eq!(nodes[1].depth, 2);
    }

    #[test]
    fn a_finished_walk_still_sees_a_deeper_qualifier_next_to_depth_one() {
        let nodes = [good(), PageNode { depth: 2, ..good() }];
        assert_eq!(
            choose_page_crop(&nodes, Some(monitor()), true, seen_direct(1)),
            PageCrop::Unresolved
        );
    }

    #[test]
    fn an_unmeasured_monitor_is_not_a_depth_one_page() {
        let filler = PageNode {
            is_group: Some(false),
            ..good()
        };
        let nodes = [filler, good()];
        let layer = seen_direct(2);
        let unmeasured = choose_page_crop(&nodes, None, false, layer);
        let measured = choose_page_crop(&nodes, Some(monitor()), false, layer);
        assert_eq!(unmeasured, PageCrop::Unresolved);
        assert_eq!(measured, PageCrop::Crop(1));
    }

    #[test]
    fn direct_children_are_visited_before_their_own_children() {
        let mut walk = BreadthWalk::seed(SiblingChain::complete(["page", "sibling"]));
        let (first, depth) = walk.pop_front().expect("page");
        assert_eq!(first, "page");
        assert_eq!(walk.record_observed(depth), 1);
        walk.enqueue_children(depth, SiblingChain::complete(["run-a", "run-b"]));
        let (second, depth) = walk.pop_front().expect("sibling");
        assert_eq!(second, "sibling");
        assert_eq!(walk.record_observed(depth), 1);
        let (third, depth) = walk.pop_front().expect("first text run");
        assert_eq!(third, "run-a");
        assert_eq!(walk.record_observed(depth), 2);
    }

    #[test]
    fn depth_one_is_all_observed_before_deeper_nodes_spend_the_budget() {
        let mut walk = BreadthWalk::seed(SiblingChain::complete([0_u32, 1]));
        let mut nodes = Vec::new();
        let mut refused_deeper = false;
        while let Some((id, depth)) = walk.pop_front() {
            match frontier(walk.visited(), depth.get(), None) {
                Frontier::BudgetLeftNodes => {
                    assert!(
                        !depth.is_direct_child(),
                        "the budget refused a direct child before depth 1 was done"
                    );
                    refused_deeper = true;
                    walk.stop_for_node_cap(depth);
                }
                Frontier::DepthLeftChildren => {
                    let stored = walk.record_observed(depth);
                    nodes.push(budget_node(id, stored));
                    walk.stop_for_depth_cap();
                }
                Frontier::Open { descend } => {
                    let stored = walk.record_observed(depth);
                    nodes.push(budget_node(id, stored));
                    if descend && id == 1 && depth.is_direct_child() {
                        walk.enqueue_children(
                            depth,
                            SiblingChain::complete(10..(10 + PAGE_WALK_NODE_CAP)),
                        );
                    }
                }
            }
        }
        let outcome = walk.outcome();
        let cap = usize::try_from(PAGE_WALK_NODE_CAP).unwrap();
        assert!(refused_deeper);
        assert!(!outcome.walk_finished);
        assert!(outcome.depth_one.is_complete());
        assert_eq!(nodes.len(), cap);
        assert_eq!(nodes[0].depth, 1);
        assert_eq!(nodes[1].depth, 1);
        assert_eq!(nodes[2].depth, 2);
        assert!(nodes[2..].iter().all(|node| node.depth == 2));
        // The deeper groups qualify. A finished count of this slice is more
        // than one page, so the unfinished crop is the depth-1 rule.
        assert_eq!(
            choose_page_crop(&nodes, Some(monitor()), true, depth_one_left_unobserved(),),
            PageCrop::Unresolved
        );
        assert_eq!(
            choose_page_crop(
                &nodes,
                Some(monitor()),
                outcome.walk_finished,
                outcome.depth_one,
            ),
            PageCrop::Crop(0)
        );
    }

    fn budget_node(id: u32, depth: u32) -> PageNode {
        let base = if depth == 1 && id == 0 {
            good()
        } else if depth == 1 {
            page_at(0.0, 0.0, 10.0, 10.0)
        } else {
            good()
        };
        PageNode { depth, ..base }
    }

    #[test]
    fn the_node_cap_leaves_a_direct_child_unobserved() {
        let mut walk = BreadthWalk::seed(SiblingChain::complete(0..(PAGE_WALK_NODE_CAP + 3)));
        let mut nodes = Vec::new();
        while let Some((id, depth)) = walk.pop_front() {
            match frontier(walk.visited(), depth.get(), None) {
                Frontier::BudgetLeftNodes => {
                    assert!(depth.is_direct_child());
                    walk.stop_for_node_cap(depth);
                }
                Frontier::DepthLeftChildren => {
                    let stored = walk.record_observed(depth);
                    nodes.push(direct_or_narrow(id, stored));
                    walk.stop_for_depth_cap();
                }
                Frontier::Open { .. } => {
                    let stored = walk.record_observed(depth);
                    nodes.push(direct_or_narrow(id, stored));
                }
            }
        }
        let outcome = walk.outcome();
        let cap = usize::try_from(PAGE_WALK_NODE_CAP).unwrap();
        assert!(!outcome.walk_finished);
        assert!(!outcome.depth_one.is_complete());
        assert_eq!(nodes.len(), cap);
        assert_eq!(nodes[0].depth, 1);
        assert_eq!(
            choose_page_crop(
                &nodes,
                Some(monitor()),
                outcome.walk_finished,
                outcome.depth_one,
            ),
            PageCrop::Unresolved
        );
        // The observed slice itself has one qualifying depth-1 page. The
        // refusal is why that page is not clipped.
        assert_eq!(
            choose_page_crop(&nodes, Some(monitor()), false, seen_direct(nodes.len())),
            PageCrop::Crop(0)
        );
    }

    fn direct_or_narrow(id: u32, depth: u32) -> PageNode {
        let base = if id == 0 {
            good()
        } else {
            page_at(0.0, 0.0, 10.0, 10.0)
        };
        PageNode { depth, ..base }
    }

    #[test]
    fn a_child_past_the_depth_cap_does_not_make_depth_one_incomplete() {
        let mut walk = BreadthWalk::seed(SiblingChain::complete([0_u32]));
        let mut last = None;
        while let Some((_id, depth)) = walk.pop_front() {
            let child_at_cap = (depth.get() >= PAGE_WALK_DEPTH_CAP).then_some(true);
            match frontier(walk.visited(), depth.get(), child_at_cap) {
                Frontier::BudgetLeftNodes => walk.stop_for_node_cap(depth),
                Frontier::DepthLeftChildren => {
                    last = Some(walk.record_observed(depth));
                    walk.stop_for_depth_cap();
                }
                Frontier::Open { descend } => {
                    last = Some(walk.record_observed(depth));
                    if descend {
                        walk.enqueue_children(depth, SiblingChain::complete([0]));
                    }
                }
            }
        }
        let outcome = walk.outcome();
        assert_eq!(last, Some(PAGE_WALK_DEPTH_CAP));
        assert!(!outcome.walk_finished);
        assert!(outcome.depth_one.is_complete());
    }

    fn record_until_empty(walk: &mut BreadthWalk<u32>) -> Vec<PageNode> {
        let mut nodes = Vec::new();
        while let Some((id, depth)) = walk.pop_front() {
            let stored = walk.record_observed(depth);
            nodes.push(direct_or_narrow(id, stored));
        }
        nodes
    }

    #[test]
    fn sibling_chain_keeps_a_failed_read_apart_from_the_end() {
        let done = sibling_chain(SiblingRead::Item(1_u32), |n| {
            if *n < 3 {
                SiblingRead::Item(n + 1)
            } else {
                SiblingRead::End
            }
        });
        assert_eq!(done.items, vec![1, 2, 3]);
        assert!(!done.truncated_by_error);

        // The read of what follows 2 failed. 1 and 2 are the prefix.
        let cut = sibling_chain(SiblingRead::Item(1_u32), |n| {
            if *n < 2 {
                SiblingRead::Item(n + 1)
            } else {
                SiblingRead::Failed
            }
        });
        assert_eq!(cut.items, vec![1, 2]);
        assert!(cut.truncated_by_error);

        let empty_end = sibling_chain(SiblingRead::End::<u32>, |_| SiblingRead::Item(1_u32));
        assert!(empty_end.items.is_empty());
        assert!(!empty_end.truncated_by_error);

        let empty_fail = sibling_chain(SiblingRead::Failed::<u32>, |_| SiblingRead::Item(1_u32));
        assert!(empty_fail.items.is_empty());
        assert!(empty_fail.truncated_by_error);
        assert_ne!(empty_end.truncated_by_error, empty_fail.truncated_by_error);
    }

    #[test]
    fn a_truncated_depth_one_prefix_with_one_qualifier_does_not_crop() {
        // Five direct children existed. The read failed after the third.
        // One of those three qualifies. The two that never came back could
        // also qualify, so this prefix must not be clipped.
        let chain = sibling_chain(SiblingRead::Item(0_u32), |id| {
            if *id < 2 {
                SiblingRead::Item(id + 1)
            } else {
                SiblingRead::Failed
            }
        });
        assert_eq!(chain.items, vec![0, 1, 2]);
        assert!(chain.truncated_by_error);
        let mut walk = BreadthWalk::seed(chain);
        let nodes = record_until_empty(&mut walk);
        let outcome = walk.outcome();
        assert_eq!(nodes.len(), 3);
        let decision = choose_page_crop(
            &nodes,
            Some(monitor()),
            outcome.walk_finished,
            outcome.depth_one,
        );
        assert_eq!(decision, PageCrop::Unresolved);
        assert!(!outcome.walk_finished);
        assert!(!outcome.depth_one.is_complete());
        // Counts match and the cap did not fire. Only the failed read keeps
        // the layer incomplete. Dropping that argument would report it seen.
        assert!(!DepthOneComplete::from_walk(3, 3, false, true).is_complete());
        assert!(DepthOneComplete::from_walk(3, 3, false, false).is_complete());
        // The same prefix would clip if the walk reported the layer complete.
        assert_eq!(
            choose_page_crop(&nodes, Some(monitor()), false, seen_direct(nodes.len())),
            PageCrop::Crop(0)
        );
    }

    #[test]
    fn a_complete_depth_one_chain_with_one_qualifier_still_crops() {
        let chain = sibling_chain(SiblingRead::Item(0_u32), |id| {
            if *id < 2 {
                SiblingRead::Item(id + 1)
            } else {
                SiblingRead::End
            }
        });
        assert_eq!(chain.items, vec![0, 1, 2]);
        assert!(!chain.truncated_by_error);
        let mut walk = BreadthWalk::seed(chain);
        let nodes = record_until_empty(&mut walk);
        let outcome = walk.outcome();
        assert_eq!(nodes.len(), 3);
        assert_eq!(
            choose_page_crop(
                &nodes,
                Some(monitor()),
                outcome.walk_finished,
                outcome.depth_one,
            ),
            PageCrop::Crop(0)
        );
        assert!(outcome.walk_finished);
        assert!(outcome.depth_one.is_complete());
    }

    #[test]
    fn a_truncated_deeper_chain_still_crops_the_one_depth_one_page() {
        // Depth 1 is the whole list and exactly one of those nodes qualifies.
        // The depth-2 read failed after one child, and that child also
        // qualifies. A finished count of this slice is two pages. The
        // depth-1 answer stays the one depth-1 page.
        let mut walk = BreadthWalk::seed(sibling_chain(SiblingRead::Item(0_u32), |id| {
            if *id < 2 {
                SiblingRead::Item(id + 1)
            } else {
                SiblingRead::End
            }
        }));
        let mut nodes = Vec::new();
        while let Some((id, depth)) = walk.pop_front() {
            let stored = walk.record_observed(depth);
            nodes.push(direct_or_narrow(id, stored));
            if depth.is_direct_child() && id == 0 {
                let deeper = sibling_chain(SiblingRead::Item(0_u32), |_| SiblingRead::Failed);
                assert_eq!(deeper.items, vec![0]);
                assert!(deeper.truncated_by_error);
                walk.enqueue_children(depth, deeper);
            }
        }
        let outcome = walk.outcome();
        assert_eq!(nodes.iter().filter(|node| node.depth == 1).count(), 3);
        assert_eq!(
            choose_page_crop(
                &nodes,
                Some(monitor()),
                outcome.walk_finished,
                outcome.depth_one,
            ),
            PageCrop::Crop(0)
        );
        assert!(!outcome.walk_finished);
        assert!(outcome.depth_one.is_complete());
    }

    #[test]
    fn a_truncated_deeper_chain_with_no_qualifier_is_not_a_finished_zero() {
        let mut walk = BreadthWalk::seed(sibling_chain(SiblingRead::Item(1_u32), |id| {
            if *id < 3 {
                SiblingRead::Item(id + 1)
            } else {
                SiblingRead::End
            }
        }));
        let mut nodes = Vec::new();
        while let Some((id, depth)) = walk.pop_front() {
            let stored = walk.record_observed(depth);
            nodes.push(direct_or_narrow(id, stored));
            if depth.is_direct_child() && id == 1 {
                let deeper = sibling_chain(SiblingRead::Item(4_u32), |_| SiblingRead::Failed);
                assert!(deeper.truncated_by_error);
                walk.enqueue_children(depth, deeper);
            }
        }
        let outcome = walk.outcome();
        let decision = choose_page_crop(
            &nodes,
            Some(monitor()),
            outcome.walk_finished,
            outcome.depth_one,
        );
        assert_eq!(decision, PageCrop::Unresolved);
        assert_ne!(decision, PageCrop::NoQualifiedPage);
        assert!(!outcome.walk_finished);
        assert!(outcome.depth_one.is_complete());
    }

    #[test]
    fn a_failed_first_child_read_is_not_a_finished_empty_page() {
        let chain = sibling_chain(SiblingRead::Failed::<u32>, |_| SiblingRead::Item(0));
        assert!(chain.items.is_empty());
        assert!(chain.truncated_by_error);
        let mut walk = BreadthWalk::seed(chain);
        let nodes = record_until_empty(&mut walk);
        let outcome = walk.outcome();
        assert!(nodes.is_empty());
        let decision = choose_page_crop(
            &nodes,
            Some(monitor()),
            outcome.walk_finished,
            outcome.depth_one,
        );
        assert_eq!(decision, PageCrop::Unresolved);
        assert_ne!(decision, PageCrop::NoQualifiedPage);
        assert!(!outcome.walk_finished);
        assert!(!outcome.depth_one.is_complete());
        // Equal zeros are a finished empty layer only when the read ended.
        assert!(!DepthOneComplete::from_walk(0, 0, false, true).is_complete());
        assert!(DepthOneComplete::from_walk(0, 0, false, false).is_complete());
    }
}
