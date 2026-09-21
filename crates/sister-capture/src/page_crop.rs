//! Which on-screen PDF page, if any, a finished walk may clip to.
//!
//! The Windows walker only records what it actually observed. This function
//! decides. It is pure so a Linux `cargo test` can run it: the COM walk in
//! `windows/text.rs` is `#[cfg(windows)]` and Windows CI does not execute it.
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
}

/// `Crop` is the only result that may clip. The other two both leave the
/// document ranges alone, and they are not the same result: a finished walk
/// that found no page is not an unfinished count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageCrop {
    /// Walk finished, the monitor was measured, and exactly one node qualified.
    /// The index is into the candidate slice the caller passed.
    Crop(usize),
    /// Walk finished, the monitor was measured, and no node qualified.
    NoQualifiedPage,
    /// The walk did not finish, the monitor was not measured, or more than one
    /// node qualified. Do not clip.
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
/// `child_at_cap` is consulted only at the depth cap. Below the cap the walk
/// enqueues the children it can read, and a missing child read is the same
/// COM error as the end of a sibling list — this function does not pretend
/// those two are different. At the cap, `Some(false)` is the measured leaf
/// and `None` means the probe did not happen.
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

/// Every sibling from `first`, inclusive, in sibling order.
///
/// `next` returning `None` ends the chain. Callers that cannot tell a failed
/// sibling read from the real end both pass `None` here; this function does
/// not invent a third state for them.
pub(crate) fn sibling_chain<T>(
    mut first: Option<T>,
    mut next: impl FnMut(&T) -> Option<T>,
) -> Vec<T> {
    let mut out = Vec::new();
    while let Some(current) = first.take() {
        first = next(&current);
        out.push(current);
    }
    out
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

pub(crate) fn choose_page_crop(
    nodes: &[PageNode],
    monitor: Option<TextRect>,
    walk_finished: bool,
) -> PageCrop {
    if !walk_finished {
        return PageCrop::Unresolved;
    }
    // No monitor rectangle means the overlap test did not run. That is not
    // "we measured a screen and nothing was on it".
    let Some(monitor) = monitor else {
        return PageCrop::Unresolved;
    };
    let mut chosen = None;
    for (index, node) in nodes.iter().enumerate() {
        if rejection(node, monitor).is_some() {
            continue;
        }
        if chosen.is_some() {
            return PageCrop::Unresolved;
        }
        chosen = Some(index);
    }
    match chosen {
        Some(index) => PageCrop::Crop(index),
        None => PageCrop::NoQualifiedPage,
    }
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
        }
    }

    fn good() -> PageNode {
        page_at(100.0, 80.0, 800.0, 600.0)
    }

    #[test]
    fn direct_children_include_every_sibling_not_only_the_first() {
        let children = sibling_chain(Some(1_u32), |n| (*n < 3).then_some(n + 1));
        assert_eq!(children, vec![1, 2, 3]);
        assert!(sibling_chain(None::<u32>, |_| Some(1)).is_empty());
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
        let decision = choose_page_crop(&[good(), good()], Some(monitor()), true);
        assert_eq!(decision, PageCrop::Unresolved);
        assert_ne!(decision, PageCrop::NoQualifiedPage);
    }

    #[test]
    fn one_qualified_page_is_not_enough_when_the_walk_did_not_finish() {
        let nodes = [good()];
        let unfinished = choose_page_crop(&nodes, Some(monitor()), false);
        // The same page with the flag flipped the other way. If the line
        // above passes only because the page itself does not qualify, this
        // one fails.
        let finished = choose_page_crop(&nodes, Some(monitor()), true);
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
        };
        assert_eq!(
            choose_page_crop(&[filler, good()], Some(monitor()), true),
            PageCrop::Crop(1)
        );
        // The floor is the constant, not "anything with a positive area".
        assert_eq!(
            choose_page_crop(&[page_at(0.0, 0.0, 200.0, 80.0)], Some(monitor()), true),
            PageCrop::Crop(0)
        );
    }

    #[test]
    fn a_finished_walk_with_no_qualified_page_is_not_an_unfinished_walk() {
        let offscreen = PageNode {
            offscreen: Some(true),
            ..good()
        };
        let finished_none = choose_page_crop(&[offscreen], Some(monitor()), true);
        let unfinished_none = choose_page_crop(&[offscreen], Some(monitor()), false);
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
            choose_page_crop(&[offscreen, good()], Some(monitor()), true),
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
            choose_page_crop(&[unread], Some(monitor()), true),
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
            choose_page_crop(&[narrow, short, good()], Some(monitor()), true),
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
            choose_page_crop(&[missing, good()], Some(monitor()), true),
            PageCrop::Crop(1)
        );
        assert_eq!(
            choose_page_crop(&[zero, good()], Some(monitor()), true),
            PageCrop::Crop(1)
        );
        assert_eq!(
            choose_page_crop(&[missing], Some(monitor()), true),
            PageCrop::NoQualifiedPage
        );
        assert_eq!(
            choose_page_crop(&[zero], Some(monitor()), true),
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
            choose_page_crop(&[elsewhere, good()], Some(monitor()), true),
            PageCrop::Crop(1)
        );
        // Hangs off the left edge and still meets the monitor. `overlaps`,
        // not "fully inside".
        let partial = page_at(-50.0, 0.0, 200.0, 80.0);
        assert_eq!(rejection(&partial, monitor()), None);
        assert_eq!(
            choose_page_crop(&[partial], Some(monitor()), true),
            PageCrop::Crop(0)
        );
    }

    #[test]
    fn an_unmeasured_monitor_is_not_a_measured_screen_nothing_overlaps() {
        let unmeasured = choose_page_crop(&[good()], None, true);
        let measured_empty = choose_page_crop(
            &[good()],
            Some(TextRect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            }),
            true,
        );
        assert_eq!(unmeasured, PageCrop::Unresolved);
        assert_eq!(measured_empty, PageCrop::NoQualifiedPage);
        assert_ne!(unmeasured, measured_empty);
    }
}
