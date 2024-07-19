use std::{collections::HashMap, ffi::c_void, mem};

use accessibility::{AXUIElement, AXUIElementAttributes};
use accessibility_sys::kAXWindowRole;
use anyhow::{anyhow, Result};
use cocoa::{
    appkit::{
        NSBackingStoreType::NSBackingStoreBuffered, NSColor, NSRunningApplication, NSWindow,
        NSWindowStyleMask,
    },
    base::{id, nil},
    foundation::{NSPoint, NSRect, NSSize, NSString},
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
                        let w = WindowWrapper::new(w.clone());
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
pub struct WindowGroup {
    layout: Layout,
    primary_column_max_windows: i32,
    primary_column_pct: u8,
    active_window_idx: Option<usize>,
    windows: Vec<WindowWrapper<AXUIElement>>,
}

#[derive(Debug)]
pub struct DisplayState {
    display_id: DisplayID,
    active_group: Option<u8>,
    groups: HashMap<u8, WindowGroup>,
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
    highlight_overlay_window: Option<id>,
    status_window: Option<id>,
}

impl WindowGroup {
    fn new(window: WindowWrapper<AXUIElement>) -> Self {
        Self {
            layout: Layout::tile_horizontal(1, 50),
            active_window_idx: Some(0),
            windows: vec![window],
            primary_column_max_windows: 1,
            primary_column_pct: 50,
        }
    }

    fn _next_window_idx(&self) -> Option<usize> {
        let num_windows = self.windows.len();

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
        let num_windows = self.windows.len();

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
        self.active_window_idx.and_then(|idx| self.windows.get(idx))
    }

    fn swap_window_prev(&mut self) {
        match (self.active_window_idx, self.prev_window_idx()) {
            (Some(idx), Some(prev_idx)) => {
                self.windows.swap(idx, prev_idx);
                self.active_window_idx = Some(prev_idx);
            }
            _ => (),
        }
    }

    fn swap_window_next(&mut self) {
        match (self.active_window_idx, self.next_window_idx()) {
            (Some(idx), Some(next_idx)) => {
                self.windows.swap(idx, next_idx);
                self.active_window_idx = Some(next_idx);
            }
            _ => (),
        }
    }

    fn pop_active_window(&mut self) -> Option<WindowWrapper<AXUIElement>> {
        match self.active_window_idx {
            Some(idx) => {
                let w = self.windows.remove(idx);
                self.active_window_idx = if self.windows.len() == 0 {
                    None
                } else {
                    Some(usize::min(idx, self.windows.len() - 1))
                };
                Some(w)
            }
            None => None,
        }
    }

    fn set_layout(&mut self, layout: Layout) {
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

    fn relayout(&self, display_id: DisplayID) -> Result<()> {
        self.layout.apply(display_id, &self.windows)
    }

    fn bring_all_to_front(&self) -> Result<()> {
        for window in self.windows.iter() {
            window.activate()?;
        }
        Ok(())
    }

    fn incr_primary_column_max_windows(&mut self) {
        self.primary_column_max_windows = i32::min(
            self.primary_column_max_windows + 1,
            self.windows.len() as i32,
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

impl DisplayState {
    fn new(display_id: DisplayID, window: WindowWrapper<AXUIElement>) -> Self {
        let mut groups = HashMap::new();
        groups.insert(1, WindowGroup::new(window));
        Self {
            display_id,
            active_group: Some(1),
            groups,
        }
    }

    fn get_active_group(&self) -> Option<&WindowGroup> {
        self.active_group.and_then(|idx| self.groups.get(&idx))
    }

    fn get_active_group_mut(&mut self) -> Option<&mut WindowGroup> {
        self.active_group.and_then(|idx| self.groups.get_mut(&idx))
    }

    fn bring_active_group_to_front(&self) -> Result<()> {
        if let Some(g) = self.get_active_group() {
            g.bring_all_to_front()?;
        }
        Ok(())
    }

    fn get_active_window(&self) -> Option<&WindowWrapper<AXUIElement>> {
        self.get_active_group().and_then(|g| g.get_active_window())
    }

    fn swap_window_prev(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.swap_window_prev()
        }
    }

    fn swap_window_next(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.swap_window_next()
        }
    }

    fn pop_active_window(&mut self) -> Option<WindowWrapper<AXUIElement>> {
        self.get_active_group_mut()
            .and_then(|g| g.pop_active_window())
    }

    fn move_active_window_to_group(&mut self, g_id: u8) {
        if let Some(w) = self.pop_active_window() {
            match self.groups.get_mut(&g_id) {
                Some(g) => {
                    if !g.windows.iter().any(|w_| w_.id() == w.id()) {
                        g.windows.insert(0, w);
                        g.active_window_idx = Some(0);
                    }
                }
                None => {
                    self.groups.insert(g_id, WindowGroup::new(w));
                }
            }
        }
    }

    fn toggle_active_window_in_group(&mut self, g_id: u8) {
        if let Some(w) = self.get_active_window().cloned() {
            let window_exists_in_another_group = self.groups.iter().any(|(g_id_2, g_2)| {
                *g_id_2 != g_id && g_2.windows.iter().any(|w_2| w_2.id() == w.id())
            });

            match self.groups.get_mut(&g_id) {
                Some(g) => {
                    match g.windows.iter().position(|w_2| w_2.id() == w.id()) {
                        Some(w_idx) if window_exists_in_another_group => {
                            // Only remove the window if it is present in another group (prevent
                            // orphan windows).
                            g.windows.remove(w_idx);
                            g.active_window_idx = if g.windows.len() == 0 {
                                None
                            } else {
                                Some(usize::min(w_idx, g.windows.len() - 1))
                            };
                        }
                        Some(_) => (),
                        None => {
                            g.windows.insert(0, w);
                            g.active_window_idx = Some(0);
                        }
                    }
                }
                None => {
                    self.groups.insert(g_id, WindowGroup::new(w));
                }
            }
        }
    }

    fn close_active_window(&mut self) -> Result<()> {
        if let Some(window) = self.pop_active_window() {
            window.close()
        } else {
            Ok(())
        }
    }

    pub fn layout(&self) -> Option<&Layout> {
        self.get_active_group().map(|g| &g.layout)
    }

    fn set_layout_floating(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.set_layout_floating()
        }
    }

    fn set_layout_cascade(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.set_layout_cascade()
        }
    }

    fn set_layout_tile_horizontal(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.set_layout_tile_horizontal()
        }
    }

    fn relayout(&self) -> Result<()> {
        match self.get_active_group() {
            Some(g) => g.relayout(self.display_id),
            None => Ok(()),
        }
    }

    fn set_next_window_active(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.active_window_idx = g.next_window_idx();
        }
    }

    fn set_prev_window_active(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.active_window_idx = g.prev_window_idx();
        }
    }

    fn incr_primary_column_max_windows(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.incr_primary_column_max_windows()
        }
    }

    fn decr_primary_column_max_windows(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.decr_primary_column_max_windows()
        }
    }

    fn incr_primary_column_width(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.incr_primary_column_width()
        }
    }

    fn decr_primary_column_width(&mut self) {
        if let Some(g) = self.get_active_group_mut() {
            g.decr_primary_column_width()
        }
    }

    fn set_active_group(&mut self, g_id: u8) {
        self.active_group = Some(g_id);
    }
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            drag_window: None,
            mode: Mode::Insert,
            active_display_idx: None,
            display_ids: vec![],
            displays: HashMap::new(),
            minimized_windows: vec![],
            highlight_overlay_window: None,
            status_window: None,
        }
    }

    fn refresh_active_window(&mut self) {
        let active_display_id = self.displays.iter_mut().find_map(|(display_id, ds)| {
            ds.groups.iter_mut().find_map(|(g_idx, g)| {
                g.windows
                    .iter()
                    .position(|w| w.frontmost_and_main().unwrap_or(false))
                    .map(|w_idx| {
                        ds.active_group = Some(*g_idx);
                        g.active_window_idx = Some(w_idx);
                        *display_id
                    })
            })
        });
        self.active_display_idx = active_display_id
            .and_then(|display_id| self.display_ids.iter().position(|d_id| *d_id == display_id));
    }

    fn insert_open_window(&mut self, window: WindowWrapper<AXUIElement>, display_id: DisplayID) {
        match self.displays.get_mut(&display_id) {
            Some(ds) => match ds.get_active_group_mut() {
                Some(g) => {
                    g.windows.insert(0, window);
                    g.active_window_idx = Some(0);
                }
                None => {
                    ds.groups.insert(0, WindowGroup::new(window));
                    ds.active_group = Some(0);
                }
            },
            None => {
                self.displays
                    .insert(display_id, DisplayState::new(display_id, window));
            }
        }
    }

    fn window_exists(&self, window: &WindowWrapper<AXUIElement>) -> Result<bool> {
        for (_, d) in self.displays.iter() {
            for (_, g) in d.groups.iter() {
                for other in g.windows.iter() {
                    if window.is_same_window(other)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    pub fn refresh_window_list(&mut self) -> Result<()> {
        self.display_ids = CGDisplay::active_displays()
            .map_err(|e| anyhow!(format!("CGDisplay::active_displays {:?}", e)))?;

        self.displays
            .retain(|d_id, _v| self.display_ids.contains(d_id));

        let (open_windows, minimized_windows) = get_all_windows()?;

        for (_, d) in self.displays.iter_mut() {
            for (_, g) in d.groups.iter_mut() {
                g.windows = g
                    .windows
                    .drain(..)
                    .filter(|w| {
                        open_windows.iter().any(|w2| {
                            w.is_same_window(w2).unwrap_or_else(|e| {
                                eprintln!("is_same_windows: {:?}", e);
                                false
                            })
                        })
                    })
                    .collect();
            }
        }

        let my_pid = unsafe {
            let app = NSRunningApplication::currentApplication(nil);
            app.processIdentifier_()
        };
        for w in open_windows {
            if w.element().pid()? != my_pid && !self.window_exists(&w)? {
                let display_id = w.display()?.id;
                self.insert_open_window(w, display_id);
            }
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

    fn maybe_enter_normal_mode(&mut self) -> Result<()> {
        Ok(if let Mode::Insert = self.mode {
            self.refresh_window_list()?;
            self.open_status_window();
        })
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
        if let Some(w) = self.get_active_window() {
            let f = w.frame()?;
            let outset = 7.;
            let pos = position_to_origin(&w)?;
            let size = unsafe { mem::transmute::<CGSize, NSSize>(f.size) };
            let rect = NSRect::new(pos, size).inset(-outset, -outset);
            match self.highlight_overlay_window {
                None => unsafe {
                    let overlay = NSWindow::alloc(nil);
                    overlay.initWithContentRect_styleMask_backing_defer_(
                        rect,
                        NSWindowStyleMask::empty(),
                        NSBackingStoreBuffered,
                        false,
                    );
                    overlay.setBackgroundColor_(NSColor::systemRedColor(nil));
                    overlay.setAlphaValue_(0.7);
                    overlay.makeKeyAndOrderFront_(nil);
                    self.highlight_overlay_window = Some(overlay);
                },
                Some(overlay) => {
                    unsafe {
                        overlay.setContentSize_(rect.size);
                        overlay.setFrameOrigin_(rect.origin);
                        overlay.setContentSize_(rect.size);
                        overlay.makeKeyAndOrderFront_(nil);
                    };
                }
            }
        }
        self.bring_status_window_to_front();
        Ok(())
    }

    fn close_highlight_window(&mut self) {
        if let Some(window) = self.highlight_overlay_window {
            unsafe {
                window.close();
            };
            self.highlight_overlay_window = None;
        }
    }

    fn open_status_window(&mut self) {
        self.close_status_window();

        let rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(300., 200.));
        unsafe {
            let window = NSWindow::alloc(nil);
            window.initWithContentRect_styleMask_backing_defer_(
                rect,
                NSWindowStyleMask::NSTitledWindowMask | NSWindowStyleMask::NSClosableWindowMask,
                NSBackingStoreBuffered,
                false,
            );
            let title = NSString::alloc(nil).init_str("Window Manager");
            window.setTitle_(title);
            window.setAlphaValue_(0.7);
            window.center();
            self.status_window = Some(window);
        }
    }

    fn bring_status_window_to_front(&self) {
        if let Some(window) = self.status_window {
            unsafe {
                window.orderFrontRegardless();
            };
        }
    }

    fn close_status_window(&mut self) {
        match self.status_window {
            Some(window) => {
                unsafe {
                    window.close();
                };
                self.status_window = None;
            }
            None => (),
        }
    }

    fn activate_active_window(&self) -> Result<()> {
        if let Some(w) = self.get_active_window() {
            eprintln!("Activate window {:?}", w);
            w.activate()?;
        }
        Ok(())
    }

    fn bring_active_display_group_to_front(&self) -> Result<()> {
        if let Some(d) = self.get_active_display() {
            d.bring_active_group_to_front()?;
        }
        Ok(())
    }

    fn set_next_window_active(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_next_window_active()
        }
    }

    fn set_prev_window_active(&mut self) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_prev_window_active();
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

    fn move_active_window_to_group(&mut self, g_id: u8) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.move_active_window_to_group(g_id)
        }
    }

    fn toggle_active_window_in_group(&mut self, g_id: u8) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.toggle_active_window_in_group(g_id)
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
        self.get_active_display().and_then(|ds| ds.layout())
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

    fn set_active_display_group(&mut self, g_id: u8) {
        if let Some(ds) = self.get_active_display_mut() {
            ds.set_active_group(g_id);
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
                self.maybe_enter_normal_mode()?;
                self.highlight_active_window()?;
                Ok(())
            }
            ModeInsert => {
                self.set_mode(Mode::Insert);
                self.close_highlight_window();
                self.close_status_window();
                Ok(())
            }
            ModeInsertNormal => {
                self.set_mode(Mode::InsertNormal);
                self.refresh_window_list()?;
                self.open_status_window();
                self.highlight_active_window()?;
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
                self.maybe_enter_normal_mode()?;
                self.set_next_window_active();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            PrevWindow => {
                self.maybe_enter_normal_mode()?;
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
                self.maybe_enter_normal_mode()?;
                self.set_next_display_active();
                self.activate_active_window()?;
                self.close_status_window();
                self.open_status_window();
                self.highlight_active_window()?;
                Ok(())
            }
            PrevDisplay => {
                self.maybe_enter_normal_mode()?;
                self.set_prev_display_active();
                self.activate_active_window()?;
                self.close_status_window();
                self.open_status_window();
                self.highlight_active_window()?;
                Ok(())
            }
            MoveWindowToNextDisplay => {
                self.move_active_window_to_next_display();
                self.set_next_display_active();
                self.relayout_all()?;
                self.close_status_window();
                self.open_status_window();
                self.activate_active_window()?;
                self.highlight_active_window()?;
                Ok(())
            }
            MoveWindowToPrevDisplay => {
                self.move_active_window_to_prev_display();
                self.set_prev_display_active();
                self.relayout_all()?;
                self.close_status_window();
                self.open_status_window();
                self.highlight_active_window()?;
                Ok(())
            }
            ShowGroup(g_idx) => {
                self.maybe_enter_normal_mode()?;
                self.set_active_display_group(*g_idx);
                self.bring_active_display_group_to_front()?;
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            MoveWindowToGroup(g_id) => {
                self.move_active_window_to_group(*g_id);
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
            ToggleWindowInGroup(g_id) => {
                self.toggle_active_window_in_group(*g_id);
                self.activate_active_window()?;
                self.relayout()?;
                self.highlight_active_window()?;
                Ok(())
            }
        }
    }
}
