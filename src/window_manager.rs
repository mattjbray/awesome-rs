use std::{collections::HashMap, ffi::c_void, mem};

use accessibility::{AXUIElement, AXUIElementAttributes};
use accessibility_sys::kAXWindowRole;
use anyhow::{anyhow, Result};
use cocoa::{
    appkit::{NSBackingStoreType::NSBackingStoreBuffered, NSColor, NSWindow, NSWindowStyleMask},
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize},
};
use core_foundation::{
    array::CFArray,
    base::{FromVoid, ItemRef, TCFType, ToVoid},
    dictionary::CFDictionary,
    number::CFNumber,
    string::CFString,
};
use core_graphics::{
    display::{kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, CGDisplay},
    geometry::{CGPoint, CGRect, CGSize},
    window::{kCGWindowLayer, kCGWindowOwnerPID},
};

use crate::{
    action::Action,
    drag_window::DragWindow,
    layout::Layout,
    mode::Mode,
    window::{Window, WindowWrapper},
    CGErrorWrapper,
};

fn get_window_pids(on_screen_only: bool) -> Result<Vec<i64>> {
    let opts = kCGWindowListExcludeDesktopElements;
    let opts = if on_screen_only {
        opts | kCGWindowListOptionOnScreenOnly
    } else {
        opts
    };
    let window_list: CFArray<*const c_void> =
        CGDisplay::window_list_info(opts, None).ok_or(anyhow!("no window_list_info"))?;

    let iter = window_list
        .iter()
        .map(|w| unsafe { CFDictionary::from_void(*w) })
        .filter(|d: &ItemRef<CFDictionary>| {
            // Keep only windows at layer 0
            let l: CFString = unsafe { CFString::wrap_under_create_rule(kCGWindowLayer) };
            let layer_void: ItemRef<'_, *const c_void> = d.get(l.to_void());
            let layer = unsafe { CFNumber::from_void(*layer_void) };
            layer.to_i32() == Some(0)
        })
        .filter_map(|d| {
            eprintln!("{:?}", d);
            let k: CFString = unsafe { CFString::wrap_under_create_rule(kCGWindowOwnerPID) };
            let pid = d.get(k.to_void());
            let pid = unsafe { CFNumber::from_void(*pid) };
            pid.to_i64()
        })
        .collect::<Vec<i64>>();
    Ok(iter)
}

fn get_all_windows() -> Result<(
    Vec<WindowWrapper<AXUIElement>>,
    Vec<WindowWrapper<AXUIElement>>,
)> {
    let mut window_pids_deduped = vec![];
    // First use onScreenOnly to get apps with recent windows first
    for &pid in get_window_pids(true)?.iter() {
        if !window_pids_deduped.contains(&pid) {
            window_pids_deduped.push(pid);
        }
    }
    // Then get everything else to get apps with minimized
    for &pid in get_window_pids(false)?.iter() {
        if !window_pids_deduped.contains(&pid) {
            window_pids_deduped.push(pid);
        }
    }

    let apps = window_pids_deduped
        .iter()
        .map(|pid| AXUIElement::application(*pid as i32))
        .collect::<Vec<_>>();

    let mut open_windows = vec![];
    let mut minimized_windows = vec![];
    for app in apps {
        match app.windows() {
            Ok(windows) => {
                for w in windows.iter() {
                    if w.role()? == kAXWindowRole {
                        let w = WindowWrapper(w.clone());
                        // w.debug_attributes()?;
                        if w.minimized()? {
                            minimized_windows.push(w);
                        } else {
                            open_windows.push(w);
                        }
                    }
                }
            }
            Err(accessibility::Error::Ax(accessibility_sys::kAXErrorCannotComplete)) => {
                // e.g. kCGWindowOwnerName="Window Server" kCGWindowName=StatusIndicator
                ()
            }
            Err(e) => return Err(e.into()),
        }
    }

    eprintln!("open windows: {:?}", open_windows);
    eprintln!("minimized windows: {:?}", minimized_windows);

    Ok((open_windows, minimized_windows))
}

/// Return the position of the bottom-left of the window in Cocoa coordinates:
/// (0,0) is bottom-left of main display, y increases in the up direction.
fn position_to_origin(w: &WindowWrapper<AXUIElement>) -> Result<NSPoint> {
    // (0,0) is top-left of main display, y increases down the screen
    let f = w.frame()?;
    let m = CGDisplay::main().bounds();

    // (0,0) is bottom-left of main display, y increases up the screen
    let x = f.origin.x;
    let y = m.size.height - f.origin.y - f.size.height;
    Ok(NSPoint::new(x, y))
}

type DisplayID = u32;

#[derive(Debug)]
pub struct DisplayState {
    display_id: DisplayID,
    layout: Layout,
    active_window_idx: Option<usize>,
    open_windows: Vec<WindowWrapper<AXUIElement>>,
    primary_column_max_windows: i32,
    primary_column_pct: u8,
}

impl DisplayState {
    fn new(display_id: DisplayID, window: WindowWrapper<AXUIElement>) -> Self {
        Self {
            display_id,
            layout: Layout::floating(),
            active_window_idx: Some(0),
            open_windows: vec![window],
            primary_column_max_windows: 1,
            primary_column_pct: 50,
        }
    }

    fn _next_window_idx(&self) -> Option<usize> {
        let num_windows = self.open_windows.len();

        if num_windows == 0 {
            None
        } else {
            match self.active_window_idx {
                Some(idx) => {
                    if idx >= num_windows - 1 {
                        Some(0)
                    } else {
                        Some(idx + 1)
                    }
                }
                None => Some(0),
            }
        }
    }

    fn _prev_window_idx(&self) -> Option<usize> {
        let num_windows = self.open_windows.len();

        if num_windows == 0 {
            None
        } else {
            match self.active_window_idx {
                Some(idx) => {
                    if idx == 0 {
                        Some(num_windows - 1)
                    } else {
                        Some(idx - 1)
                    }
                }
                None => Some(0),
            }
        }
    }

    fn next_window_idx(&self) -> Option<usize> {
        match self.layout {
            Layout::TileHorizontal(_) => self._next_window_idx(),
            _ => self._prev_window_idx(),
        }
    }

    fn prev_window_idx(&self) -> Option<usize> {
        match self.layout {
            Layout::TileHorizontal(_) => self._prev_window_idx(),
            _ => self._next_window_idx(),
        }
    }

    fn get_active_window(&self) -> Option<&WindowWrapper<AXUIElement>> {
        self.active_window_idx
            .map(|idx| self.open_windows.get(idx).unwrap())
    }

    fn swap_window_prev(&mut self) {
        match (self.active_window_idx, self.prev_window_idx()) {
            (Some(idx), Some(prev_idx)) => {
                self.open_windows.swap(idx, prev_idx);
                self.active_window_idx = Some(prev_idx);
            }
            _ => (),
        }
    }

    fn swap_window_next(&mut self) {
        match (self.active_window_idx, self.next_window_idx()) {
            (Some(idx), Some(next_idx)) => {
                self.open_windows.swap(idx, next_idx);
                self.active_window_idx = Some(next_idx);
            }
            _ => (),
        }
    }

    fn pop_active_window(&mut self) -> Option<WindowWrapper<AXUIElement>> {
        match self.active_window_idx {
            Some(idx) => {
                let w = self.open_windows.remove(idx);
                self.active_window_idx = if self.open_windows.len() == 0 {
                    None
                } else {
                    Some(usize::min(idx, self.open_windows.len() - 1))
                };
                Some(w)
            }
            None => None,
        }
    }

    fn close_active_window(&mut self) -> Result<()> {
        if let Some(window) = self.pop_active_window() {
            window.close()
        } else {
            Ok(())
        }
    }

    fn set_layout(&mut self, layout: Layout) {
        eprintln!("set_layout: {:?} for display {:?}", layout, self.display_id);
        self.layout = layout;
    }

    fn set_layout_floating(&mut self) {
        self.set_layout(Layout::floating())
    }

    fn set_layout_cascade(&mut self) {
        self.set_layout(Layout::cascade())
    }

    fn set_layout_tile_horizontal(&mut self) {
        self.set_layout(Layout::tile_horizontal(
            self.primary_column_max_windows,
            self.primary_column_pct,
        ))
    }

    fn relayout(&self) -> Result<()> {
        self.layout.apply(self.display_id, &self.open_windows)
    }

    fn incr_primary_column_max_windows(&mut self) {
        self.primary_column_max_windows = i32::min(
            self.primary_column_max_windows + 1,
            self.open_windows.len() as i32,
        );
        self.set_layout_tile_horizontal();
    }

    fn decr_primary_column_max_windows(&mut self) {
        self.primary_column_max_windows = i32::max(self.primary_column_max_windows - 1, 1);
        self.set_layout_tile_horizontal();
    }

    fn incr_primary_column_width(&mut self) {
        if self.primary_column_pct <= 80 {
            self.primary_column_pct += 10;
        }
        self.set_layout_tile_horizontal();
    }

    fn decr_primary_column_width(&mut self) {
        if self.primary_column_pct >= 20 {
            self.primary_column_pct -= 10;
        }
        self.set_layout_tile_horizontal();
    }
}

#[derive(Debug)]
pub struct WindowManager {
    drag_window: Option<DragWindow>,
    mode: Mode,
    active_display_idx: Option<usize>,
    /// Index into self.display_ids
    display_ids: Vec<DisplayID>,
    displays: HashMap<DisplayID, DisplayState>,
    minimized_windows: Vec<WindowWrapper<AXUIElement>>,
    ns_window: Option<id>,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            drag_window: None,
            mode: Mode::Normal,
            active_display_idx: None,
            display_ids: vec![],
            displays: HashMap::new(),
            minimized_windows: vec![],
            ns_window: None,
        }
    }

    fn refresh_active_window(&mut self) {
        let active_display_id = self.displays.iter_mut().find_map(|(display_id, ds)| {
            ds.open_windows
                .iter()
                .position(|w| w.frontmost_and_main().unwrap_or(false))
                .map(|idx| {
                    ds.active_window_idx = Some(idx);
                    *display_id
                })
        });
        self.active_display_idx = active_display_id
            .and_then(|display_id| self.display_ids.iter().position(|d_id| *d_id == display_id));
    }

    fn insert_open_window(&mut self, window: WindowWrapper<AXUIElement>, display_id: DisplayID) {
        match self.displays.get_mut(&display_id) {
            Some(ds) => {
                ds.open_windows.insert(0, window);
                ds.active_window_idx = Some(0)
            }
            None => {
                self.displays
                    .insert(display_id, DisplayState::new(display_id, window));
            }
        }
    }

    pub fn refresh_window_list(&mut self) -> Result<()> {
        self.display_ids = CGDisplay::active_displays()
            .map_err(|e| anyhow!(format!("CGDisplay::active_displays {:?}", e)))?;
        let (open_windows, minimized_windows) = get_all_windows()?;
        for display in self.displays.values_mut() {
            display.open_windows.clear();
        }
        for w in open_windows {
            let display_id = w.display()?.id;
            self.insert_open_window(w, display_id);
        }
        self.minimized_windows = minimized_windows;
        self.refresh_active_window();
        Ok(())
    }

    pub fn drag_window(&self) -> Option<&DragWindow> {
        self.drag_window.as_ref()
    }

    pub fn set_drag_window(&mut self, dw: Option<DragWindow>) {
        self.drag_window = dw
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn is_normal_mode(&self) -> bool {
        self.mode == Mode::Normal
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        println!("Entered {:?} mode", self.mode);
    }

    fn get_active_display(&self) -> Option<&DisplayState> {
        self.active_display_idx.and_then(|idx| {
            let display_id = self.display_ids[idx];
            self.displays.get(&display_id)
        })
    }

    fn get_active_display_mut(&mut self) -> Option<&mut DisplayState> {
        self.active_display_idx.and_then(|idx| {
            let display_id = self.display_ids[idx];
            self.displays.get_mut(&display_id)
        })
    }

    fn get_active_window(&self) -> Option<&WindowWrapper<AXUIElement>> {
        self.get_active_display()
            .and_then(|ds| ds.get_active_window())
    }

    /// Create a window slightly larger than and behind the active window.
    fn highlight_active_window(&mut self) -> Result<()> {
        self.remove_highlight_window();
        match self.get_active_window() {
            Some(w) => {
                let f = w.frame()?;
                let outset = 7.;
                let pos = position_to_origin(&w)?;
                let size = unsafe { mem::transmute::<CGSize, NSSize>(f.size) };
                let rect = NSRect::new(pos, size).inset(-outset, -outset);

                unsafe {
                    let window = NSWindow::alloc(nil);
                    window.initWithContentRect_styleMask_backing_defer_(
                        rect,
                        NSWindowStyleMask::empty(),
                        NSBackingStoreBuffered,
                        false,
                    );
                    window.setBackgroundColor_(NSColor::systemRedColor(nil));
                    window.setAlphaValue_(0.7);
                    window.makeKeyAndOrderFront_(nil);
                    self.ns_window = Some(window);
                }
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn remove_highlight_window(&mut self) {
        match self.ns_window {
            Some(window) => {
                unsafe {
                    window.close();
                };
                self.ns_window = None;
            }
            None => (),
        }
    }

    fn activate_active_window(&self) -> Result<()> {
        if let Some(w) = self.get_active_window() {
            eprintln!("Activate window {:?}", w);
            w.activate()
        } else {
            Ok(())
        }
    }

    fn set_next_window_active(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.active_window_idx = ds.next_window_idx();
        }
    }

    fn set_prev_window_active(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.active_window_idx = ds.prev_window_idx();
        }
    }

    fn next_display_idx(&self) -> Option<usize> {
        let num_displays = self.display_ids.len();

        match self.active_display_idx {
            Some(idx) => {
                if idx >= num_displays - 1 {
                    Some(0)
                } else {
                    Some(idx + 1)
                }
            }
            None if num_displays > 0 => Some(0),
            None => None,
        }
    }

    fn set_next_display_active(&mut self) {
        self.active_display_idx = self.next_display_idx();
    }

    fn prev_display_idx(&mut self) -> Option<usize> {
        let num_displays = self.display_ids.len();

        match self.active_display_idx {
            Some(idx) => {
                if idx == 0 {
                    Some(num_displays - 1)
                } else {
                    Some(idx - 1)
                }
            }
            None if num_displays > 0 => Some(0),
            None => None,
        }
    }

    fn set_prev_display_active(&mut self) {
        self.active_display_idx = self.prev_display_idx();
    }

    fn swap_window_prev(&mut self) {
        match self.get_active_display_mut() {
            Some(ds) => ds.swap_window_prev(),
            None => (),
        }
    }

    fn swap_window_next(&mut self) {
        match self.get_active_display_mut() {
            Some(ds) => ds.swap_window_next(),
            None => (),
        }
    }

    fn move_active_window_to_display_idx(&mut self, display_idx: usize) {
        if display_idx >= self.display_ids.len() {
            return;
        }
        match self.get_active_display_mut() {
            Some(ds) => match ds.pop_active_window() {
                None => (),
                Some(window) => {
                    let display_id = self.display_ids[display_idx];
                    self.insert_open_window(window, display_id);
                }
            },
            _ => (),
        }
    }

    fn move_active_window_to_next_display(&mut self) {
        match self.next_display_idx() {
            Some(next_display_idx) => self.move_active_window_to_display_idx(next_display_idx),
            None => (),
        }
    }

    fn move_active_window_to_prev_display(&mut self) {
        match self.prev_display_idx() {
            Some(prev_display_idx) => self.move_active_window_to_display_idx(prev_display_idx),
            None => (),
        }
    }

    fn set_active_window_full(&self) -> Result<()> {
        if let Some(window) = self.get_active_window() {
            let display = window.display()?;
            window.set_frame(display.bounds())?;
        }
        Ok(())
    }

    fn set_active_window_left(&mut self) -> Result<()> {
        if let Some(window) = self.get_active_window() {
            let d = window.display()?.bounds();
            let w = window.frame()?;
            if d.origin.x > 0. && w.origin.x == d.origin.x && w.size.width == d.size.width / 2. {
                // Already at left: move to previous display.
                let pos = CGPoint::new(d.origin.x - 1.0, d.origin.y);
                let (displays, _) =
                    CGDisplay::displays_with_point(pos, 1).map_err(|e| CGErrorWrapper(e))?;

                if let Some(display_id) = displays.first() {
                    let d = CGDisplay::new(*display_id).bounds();
                    window.set_frame(CGRect::new(
                        &CGPoint::new(d.origin.x + d.size.width / 2., d.origin.y),
                        &CGSize::new(d.size.width / 2., d.size.height),
                    ))?;
                    if let Some(ds) = self.get_active_display_mut() {
                        if let Some(w) = ds.pop_active_window() {
                            let display_id = w.display()?.id;
                            self.insert_open_window(w, display_id);
                            self.active_display_idx =
                                self.display_ids.iter().position(|d_id| *d_id == display_id);
                        }
                    }
                }
            } else {
                window.set_frame(CGRect::new(
                    &d.origin,
                    &CGSize::new(d.size.width / 2., d.size.height),
                ))?;
            }
        }
        Ok(())
    }

    fn set_active_window_right(&mut self) -> Result<()> {
        if let Some(window) = self.get_active_window() {
            let d = window.display()?.bounds();
            let w = window.frame()?;
            if w.origin.x == d.origin.x + d.size.width / 2. && w.size.width == d.size.width / 2. {
                let pos = CGPoint::new(d.origin.x + d.size.width + 1.0, d.origin.y);
                let (displays, _) =
                    CGDisplay::displays_with_point(pos, 1).map_err(CGErrorWrapper)?;
                if let Some(display_id) = displays.first() {
                    let d = CGDisplay::new(*display_id).bounds();
                    window.set_frame(CGRect::new(
                        &d.origin,
                        &CGSize::new(d.size.width / 2., d.size.height),
                    ))?;
                    if let Some(ds) = self.get_active_display_mut() {
                        if let Some(w) = ds.pop_active_window() {
                            let display_id = w.display()?.id;
                            self.insert_open_window(w, display_id);
                            self.active_display_idx =
                                self.display_ids.iter().position(|d_id| *d_id == display_id);
                        }
                    }
                }
            } else {
                window.set_frame(CGRect::new(
                    &CGPoint::new(d.origin.x + d.size.width / 2., d.origin.y),
                    &CGSize::new(d.size.width / 2., d.size.height),
                ))?;
            }
        }
        Ok(())
    }

    fn minimize_active_window(&mut self) -> Result<()> {
        if let Some(ds) = self.get_active_display_mut() {
            match ds.pop_active_window() {
                Some(window) => {
                    window.set_minimized(true)?;
                    self.minimized_windows.push(window);
                    Ok(())
                }
                None => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    fn unminimize_window(&mut self) -> Result<()> {
        if let Some(window) = self.minimized_windows.pop() {
            window.set_minimized(false)?;
            let display_id = window.display()?.id;
            self.insert_open_window(window, display_id);
            Ok(())
        } else {
            Ok(())
        }
    }

    fn close_active_window(&mut self) -> Result<()> {
        match self.get_active_display_mut() {
            Some(ds) => ds.close_active_window(),
            None => Ok(()),
        }
    }

    pub fn layout(&self) -> Option<&Layout> {
        self.get_active_display().map(|ds| &ds.layout)
    }

    fn set_layout_floating(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_layout_floating()
        }
    }

    fn set_layout_cascade(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_layout_cascade()
        }
    }

    fn set_layout_tile_horizontal(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_layout_tile_horizontal()
        }
    }

    fn relayout(&self) -> Result<()> {
        if let Some(ds) = self.get_active_display() {
            ds.relayout()
        } else {
            Ok(())
        }
    }

    fn relayout_all(&self) -> Result<()> {
        for ds in self.displays.values() {
            ds.relayout()?;
        }
        Ok(())
    }

    fn incr_primary_column_max_windows(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.incr_primary_column_max_windows()
        }
    }

    fn decr_primary_column_max_windows(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.decr_primary_column_max_windows()
        }
    }

    fn incr_primary_column_width(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.incr_primary_column_width()
        }
    }

    fn decr_primary_column_width(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.decr_primary_column_width()
        }
    }

    pub fn do_action(&mut self, action: &Action) -> Result<()> {
        use Action::*;
        match action {
            RelayoutAll => {
                self.refresh_window_list()?;
                self.relayout_all()?;
                self.highlight_active_window()?;
                Ok(())
            }
            ModeNormal => {
                self.set_mode(Mode::Normal);
                self.refresh_window_list()?;
                self.highlight_active_window()?;
                Ok(())
            }
            ModeInsert => {
                self.set_mode(Mode::Insert);
                self.remove_highlight_window();
                Ok(())
            }
            LayoutFloating => {
                self.set_layout_floating();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            LayoutCascade => {
                self.set_layout_cascade();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            LayoutTiling => {
                self.set_layout_tile_horizontal();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowFull => {
                self.set_active_window_full()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowLeftHalf => {
                self.set_active_window_left()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowRightHalf => {
                self.set_active_window_right()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowMinimize => {
                self.minimize_active_window()?;
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowRestore => {
                self.unminimize_window()?;
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            WindowClose => {
                self.close_active_window()?;
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            NextWindow => {
                self.set_next_window_active();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            PrevWindow => {
                self.set_prev_window_active();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            SwapNextWindow => {
                self.swap_window_next();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            SwapPrevWindow => {
                self.swap_window_prev();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            IncrPrimaryColWidth => {
                self.incr_primary_column_width();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            DecrPrimaryColWidth => {
                self.decr_primary_column_width();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            IncrPrimaryColWindows => {
                self.incr_primary_column_max_windows();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            DecrPrimaryColWindows => {
                self.decr_primary_column_max_windows();
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            NextDisplay => {
                self.set_next_display_active();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            PrevDisplay => {
                self.set_prev_display_active();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            MoveWindowToNextDisplay => {
                self.move_active_window_to_next_display();
                self.set_next_display_active();
                self.relayout_all()?;
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            MoveWindowToPrevDisplay => {
                self.move_active_window_to_prev_display();
                self.set_prev_display_active();
                self.relayout_all()?;
                self.highlight_active_window()?;
                Ok(())
            }
        }
    }
}
