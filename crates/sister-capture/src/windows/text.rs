//! Focused, visible, non-password Edit/Document text, or a Group scoped to its
//! direct Document parent. No full-document/tree dump, ValuePattern fallback,
//! focus changes, content cache or event/keystroke listener.
use crate::assistive::{self, ReadWindow, TextEnd, TextRange, TextRect, TextRole, VisibleText};
use crate::page_crop::{
    BreadthWalk, Frontier, PAGE_WALK_DEPTH_CAP, PageCrop, PageNode, choose_page_crop, frontier,
    sibling_chain,
};
use sister_core::model::AssistiveBlock;
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        System::{
            Com::SAFEARRAY,
            Ole::{
                SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
                SafeArrayGetLBound, SafeArrayGetUBound,
            },
        },
        UI::{
            Accessibility::{
                IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
                IUIAutomationTextRange, IUIAutomationTextRangeArray, TextPatternRangeEndpoint,
                TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
                UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_GroupControlTypeId,
                UIA_TextPatternId,
            },
            WindowsAndMessaging::GetForegroundWindow,
        },
    },
    core::Interface,
};

pub(super) fn read(
    automation: &IUIAutomation,
    hwnd: HWND,
    pid: u32,
    window: &ReadWindow,
) -> Vec<AssistiveBlock> {
    let Ok(root) = (unsafe { automation.ElementFromHandle(hwnd) }) else {
        return Vec::new();
    };
    let Ok(element) = (unsafe { automation.GetFocusedElement() }) else {
        return Vec::new();
    };
    let group = unsafe { element.CurrentControlType() }.ok() == Some(UIA_GroupControlTypeId);
    let mut source = FocusedText {
        automation,
        hwnd,
        pid,
        root,
        element,
        ranges: None,
        scope: None,
        provider: None,
        group,
    };
    assistive::collect(&mut source, window)
}
struct FocusedText<'a> {
    automation: &'a IUIAutomation,
    hwnd: HWND,
    pid: u32,
    root: IUIAutomationElement,
    element: IUIAutomationElement,
    ranges: Option<IUIAutomationTextRangeArray>,
    scope: Option<IUIAutomationTextRange>,
    provider: Option<IUIAutomationElement>,
    group: bool,
}
impl FocusedText<'_> {
    fn range_in_frame(&self, range: &IUIAutomationTextRange) -> Option<bool> {
        if !self.group {
            return Some(true);
        }
        let (_, monitor) = super::screen::focused_monitor(self.hwnd)?;
        let root = unsafe { self.root.CurrentBoundingRectangle() }.ok()?;
        range_inside(range, monitor, root)
    }
    fn matches(&self) -> Option<bool> {
        unsafe {
            if GetForegroundWindow() != self.hwnd
                || super::focus::process_id(self.hwnd) != Some(self.pid)
            {
                return Some(false);
            }
            let focused = self.automation.GetFocusedElement().ok()?;
            if !self
                .automation
                .CompareElements(&focused, &self.element)
                .ok()?
                .as_bool()
            {
                return Some(false);
            }
            super::uia::belongs_to_root(self.automation, &self.element, &self.root)?;
            if let Some(provider) = &self.provider {
                super::uia::belongs_to_root(self.automation, &self.element, provider)?;
                super::uia::belongs_to_root(self.automation, provider, &self.root)?;
                if provider.CurrentControlType().ok()? != UIA_DocumentControlTypeId
                    || provider.CurrentIsPassword().ok()?.as_bool()
                    || provider.CurrentIsOffscreen().ok()?.as_bool()
                {
                    return Some(false);
                }
                if self.group {
                    let parent = self
                        .automation
                        .ControlViewWalker()
                        .ok()?
                        .GetParentElement(&self.element)
                        .ok()?;
                    if !self
                        .automation
                        .CompareElements(&parent, provider)
                        .ok()?
                        .as_bool()
                    {
                        return Some(false);
                    }
                }
            }
            let (_, monitor) = super::screen::focused_monitor(self.hwnd)?;
            let bounds = self.element.CurrentBoundingRectangle().ok()?;
            // Another monitor's text cannot use this frame as its evidence.
            if self.group {
                let root = self.root.CurrentBoundingRectangle().ok()?;
                return Some(
                    rect(bounds).overlaps(rect(monitor)) && rect(bounds).overlaps(rect(root)),
                );
            }
            Some(
                bounds.right > bounds.left
                    && bounds.bottom > bounds.top
                    && bounds.left >= monitor.left
                    && bounds.top >= monitor.top
                    && bounds.right <= monitor.right
                    && bounds.bottom <= monitor.bottom,
            )
        }
    }
}
impl FocusedText<'_> {
    /// The Document range to clip to when one Group is the on-screen page.
    ///
    /// Direct children of the focused Document are queued at depth 1 and
    /// visited breadth-first, before any node they contain. The Document
    /// itself is not a candidate. `nodes` and `elements` are appended
    /// together; the crop index is into that pair. A node the budget refuses
    /// is appended to neither.
    ///
    /// A finished walk clips when exactly one observed node qualifies. An
    /// unfinished walk also clips when every direct child was observed and
    /// exactly one of those depth-1 nodes qualifies. A direct child the node
    /// cap did not observe leaves that layer incomplete, and then an
    /// unfinished walk does not clip. A child past the depth cap leaves the
    /// walk unfinished without erasing a depth-1 layer already observed.
    /// Zero qualified pages is not an unfinished count, and it does not clip.
    ///
    /// Group, offscreen, monitor overlap, and the thumbnail/sidebar size floor
    /// are decided in `crate::page_crop`. The floor is not the page's real size.
    fn onscreen_page_scope(
        &self,
        pattern: &IUIAutomationTextPattern,
    ) -> Option<IUIAutomationTextRange> {
        let walker = unsafe { self.automation.ControlViewWalker() }.ok()?;
        let monitor = super::screen::focused_monitor(self.hwnd).map(|(_, bounds)| rect(bounds));
        let mut walk = BreadthWalk::seed(sibling_chain(
            unsafe { walker.GetFirstChildElement(&self.element) }.ok(),
            |child| unsafe { walker.GetNextSiblingElement(child) }.ok(),
        ));
        let mut nodes = Vec::new();
        let mut elements = Vec::new();
        while let Some((current, depth)) = walk.pop_front() {
            let child_at_cap = if depth.get() >= PAGE_WALK_DEPTH_CAP {
                Some(unsafe { walker.GetFirstChildElement(&current) }.is_ok())
            } else {
                None
            };
            match frontier(walk.visited(), depth.get(), child_at_cap) {
                Frontier::BudgetLeftNodes => walk.stop_for_node_cap(depth),
                Frontier::DepthLeftChildren => {
                    // Count this node, then stop. Its children are past the cap.
                    let stored = walk.record_observed(depth);
                    nodes.push(observe_page(&current, stored));
                    elements.push(current);
                    walk.stop_for_depth_cap();
                }
                Frontier::Open { descend } => {
                    let stored = walk.record_observed(depth);
                    nodes.push(observe_page(&current, stored));
                    // `depth` is the popped node. Children are one level under it.
                    if descend {
                        walk.enqueue_children(
                            depth,
                            sibling_chain(
                                unsafe { walker.GetFirstChildElement(&current) }.ok(),
                                |child| unsafe { walker.GetNextSiblingElement(child) }.ok(),
                            ),
                        );
                    }
                    elements.push(current);
                }
            }
        }
        debug_assert_eq!(nodes.len(), elements.len());
        let outcome = walk.outcome();
        match choose_page_crop(&nodes, monitor, outcome.walk_finished, outcome.depth_one) {
            PageCrop::Crop(index) => {
                let element = elements.get(index)?;
                unsafe { pattern.RangeFromChild(element) }.ok()
            }
            PageCrop::NoQualifiedPage | PageCrop::Unresolved => None,
        }
    }
}

fn observe_page(element: &IUIAutomationElement, depth: u32) -> PageNode {
    PageNode {
        is_group: unsafe { element.CurrentControlType() }
            .ok()
            .map(|kind| kind == UIA_GroupControlTypeId),
        offscreen: unsafe { element.CurrentIsOffscreen() }
            .ok()
            .map(|value| value.as_bool()),
        bounds: unsafe { element.CurrentBoundingRectangle() }.ok().map(rect),
        depth,
    }
}
fn text_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationTextPattern> {
    unsafe {
        element
            .GetCurrentPattern(UIA_TextPatternId)
            .ok()?
            .cast()
            .ok()
    }
}
impl FocusedText<'_> {
    fn visible_pattern(&mut self) -> Option<IUIAutomationTextPattern> {
        if self.role()? == TextRole::DocumentRegion {
            // PDF pages can focus a Group directly inside their text Document.
            // Never walk past that parent to a viewer that flattens hidden pages.
            // An HTML descendant Group whose parent Document has no TextPattern
            // also stops here: borrowing the outer page would leak sibling text
            // and the dedicated `role=group` control. The HTML fixture SetFocus
            // the inner Document so the nested-Document path below can run.
            unsafe {
                let parent = self
                    .automation
                    .ControlViewWalker()
                    .ok()?
                    .GetParentElement(&self.element)
                    .ok()?;
                if parent.CurrentControlType().ok()? != UIA_DocumentControlTypeId {
                    return None;
                }
                let pattern = text_pattern(&parent)?;
                self.scope = Some(pattern.RangeFromChild(&self.element).ok()?);
                self.provider = Some(parent);
                self.group = true;
                return Some(pattern);
            }
        }
        if let Some(pattern) = text_pattern(&self.element) {
            // Edge PDF often keeps keyboard focus on the TextPattern Document.
            // GetVisibleRanges then flattens offscreen pages into the same
            // range. Clip when one on-screen page Group was singled out.
            // An unfinished count and a finished walk that found no page are
            // different answers, and neither one clips.
            if self.role()? == TextRole::Document
                && let Some(scope) = self.onscreen_page_scope(&pattern)
            {
                self.scope = Some(scope);
            }
            return Some(pattern);
        }
        // Only a positively identified Document can borrow an enclosing provider.
        // An Edit without TextPattern never falls back to the rest of its page.
        if self.role()? != TextRole::Document {
            return None;
        }
        unsafe {
            let walker = self.automation.ControlViewWalker().ok()?;
            let mut parent = self.element.clone();
            for _ in 0..8 {
                parent = walker.GetParentElement(&parent).ok()?;
                if self
                    .automation
                    .CompareElements(&parent, &self.root)
                    .ok()?
                    .as_bool()
                {
                    return None;
                }
                if parent.CurrentControlType().ok()? != UIA_DocumentControlTypeId {
                    continue;
                }
                if parent.CurrentIsPassword().ok()?.as_bool()
                    || parent.CurrentIsOffscreen().ok()?.as_bool()
                {
                    return None;
                }
                if let Some(pattern) = text_pattern(&parent) {
                    self.scope = Some(pattern.RangeFromChild(&self.element).ok()?);
                    self.provider = Some(parent);
                    return Some(pattern);
                }
            }
        }
        None
    }
}
fn native_end(end: TextEnd) -> TextPatternRangeEndpoint {
    match end {
        TextEnd::Start => TextPatternRangeEndpoint_Start,
        TextEnd::End => TextPatternRangeEndpoint_End,
    }
}
impl TextRange for IUIAutomationTextRange {
    fn compare(&self, end: TextEnd, other: &Self, other_end: TextEnd) -> Option<i32> {
        unsafe {
            self.CompareEndpoints(native_end(end), other, native_end(other_end))
                .ok()
        }
    }
    fn move_end(&self, end: TextEnd, other: &Self, other_end: TextEnd) -> Option<()> {
        unsafe {
            self.MoveEndpointByRange(native_end(end), other, native_end(other_end))
                .ok()
        }
    }
}
impl VisibleText for FocusedText<'_> {
    fn context_matches(&mut self) -> bool {
        self.matches() == Some(true)
    }
    fn role(&mut self) -> Option<TextRole> {
        match unsafe { self.element.CurrentControlType() }.ok()? {
            kind if kind == UIA_EditControlTypeId => Some(TextRole::Edit),
            kind if kind == UIA_DocumentControlTypeId => Some(TextRole::Document),
            kind if kind == UIA_GroupControlTypeId => Some(TextRole::DocumentRegion),
            _ => None,
        }
    }
    fn is_password(&mut self) -> Option<bool> {
        Some(unsafe { self.element.CurrentIsPassword() }.ok()?.as_bool())
    }
    fn is_offscreen(&mut self) -> Option<bool> {
        Some(unsafe { self.element.CurrentIsOffscreen() }.ok()?.as_bool())
    }
    fn range_count(&mut self) -> Option<usize> {
        unsafe {
            let pattern = self.visible_pattern()?;
            // https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationtextpattern-getvisibleranges
            let ranges = pattern.GetVisibleRanges().ok()?;
            let count = usize::try_from(ranges.Length().ok()?).ok()?;
            self.ranges = Some(ranges);
            Some(count)
        }
    }
    fn text(&mut self, index: usize, limit: usize) -> Option<String> {
        unsafe {
            let range = self
                .ranges
                .as_ref()?
                .GetElement(i32::try_from(index).ok()?)
                .ok()?;
            if let Some(scope) = &self.scope
                && !assistive::clip_visible(&range, scope)?
            {
                return Some(String::new());
            }
            if !self.range_in_frame(&range)? {
                return None;
            }
            let text = range.GetText(i32::try_from(limit).ok()?).ok()?.to_string();
            self.range_in_frame(&range)?.then_some(text)
        }
    }
}

fn rect(r: RECT) -> TextRect {
    TextRect {
        x: f64::from(r.left),
        y: f64::from(r.top),
        width: f64::from(r.right) - f64::from(r.left),
        height: f64::from(r.bottom) - f64::from(r.top),
    }
}
struct Rectangles(*mut SAFEARRAY);
impl Drop for Rectangles {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.0);
        }
    }
}
fn range_inside(range: &IUIAutomationTextRange, monitor: RECT, window: RECT) -> Option<bool> {
    unsafe {
        let raw = range.GetBoundingRectangles().ok()?;
        if raw.is_null() {
            return None;
        }
        let values = Rectangles(raw);
        // UIA specifies a one-dimensional double array: x, y, width, height.
        if SafeArrayGetDim(values.0) != 1 || SafeArrayGetElemsize(values.0) != 8 {
            return None;
        }
        let low = SafeArrayGetLBound(values.0, 1).ok()?;
        let high = SafeArrayGetUBound(values.0, 1).ok()?;
        let count = high.checked_sub(low)?.checked_add(1)?;
        if count <= 0 || count > 512 || count % 4 != 0 {
            return None;
        }
        for offset in (0..count).step_by(4) {
            let mut coords = [0.0_f64; 4];
            for (part, value) in coords.iter_mut().enumerate() {
                let index = low
                    .checked_add(offset)?
                    .checked_add(i32::try_from(part).ok()?)?;
                SafeArrayGetElement(values.0, &index, (value as *mut f64).cast()).ok()?;
            }
            let bounds = TextRect {
                x: coords[0],
                y: coords[1],
                width: coords[2],
                height: coords[3],
            };
            if !rect(monitor).contains(bounds) || !rect(window).contains(bounds) {
                return Some(false);
            }
        }
        Some(true)
    }
}
