use std::ffi::c_void;

use accessibility::{AXUIElement, AXUIElementAttributes};
use accessibility_sys::kAXWindowRole;
use anyhow::{anyhow, Result};
use cocoa::{
    appkit::{
        NSBackingStoreType::NSBackingStoreBuffered, NSColor, NSWindow, NSWindowStyleMask,
        NSWindowTitleVisibility,
    },
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

#[derive(Debug)]
pub struct WindowManager {
    drag_window: Option<DragWindow>,
    mode: Mode,
    layout: Layout,
    active_window_idx: Option<usize>,
    open_windows: Vec<WindowWrapper<AXUIElement>>,
    minimized_windows: Vec<WindowWrapper<AXUIElement>>,
    primary_column_max_windows: i32,
    primary_column_pct: u8,
    ns_window: Option<id>,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            drag_window: None,
            mode: Mode::Normal,
            layout: Layout::Floating,
            active_window_idx: None,
            open_windows: vec![],
            minimized_windows: vec![],
            primary_column_max_windows: 1,
            primary_column_pct: 50,
            ns_window: None,
        }
    }

    fn refresh_active_window(&mut self) {
        self.active_window_idx = self
            .open_windows
            .iter()
            .position(|w| w.frontmost_and_main().unwrap_or(false));
        if self.active_window_idx.is_none() {
            eprintln!("No active window!");
        }
    }

    pub fn refresh_window_list(&mut self) -> Result<()> {
        let (open_windows, minimized_windows) = get_all_windows()?;
        self.open_windows = open_windows;
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

    fn get_active_window(&self) -> Result<Option<&WindowWrapper<AXUIElement>>> {
        match self.active_window_idx {
            None => Ok(None),
            Some(idx) => {
                let window = self
                    .open_windows
                    .get(idx)
                    .ok_or(accessibility::Error::NotFound)?;
                Ok(Some(window))
            }
        }
    }

    fn highlight_active_window(&mut self) -> Result<()> {
        self.remove_highlight_window();
        let w = self.get_active_window()?.unwrap();
        let f = w.frame()?;
        let m = CGDisplay::main().bounds();

        let outset = 7.;
        let x = f.origin.x - outset;
        let y = m.size.height - f.origin.y - f.size.height - outset;
        let width = f.size.width + outset * 2.;
        let height = f.size.height + outset * 2.;
        let rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));

        unsafe {
            let window = NSWindow::alloc(nil);
            window.initWithContentRect_styleMask_backing_defer_(
                rect,
                NSWindowStyleMask::empty(),
                NSBackingStoreBuffered,
                false,
            );
            window.setBackgroundColor_(NSColor::systemRedColor(nil));
            window.setAlphaValue_(0.5);
            window.setOpaque_(false);
            window.setTitlebarAppearsTransparent_(true);
            window.setMovableByWindowBackground_(true);
            window.setTitleVisibility_(NSWindowTitleVisibility::NSWindowTitleHidden);
            window.makeKeyAndOrderFront_(nil);
            self.ns_window = Some(window);
        }
        Ok(())
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
        if let Some(w) = self.get_active_window()? {
            eprintln!("Activate window {:?}", w);
            w.activate()
        } else {
            Ok(())
        }
    }

    fn _next_window_idx(&mut self) -> Option<usize> {
        match self.active_window_idx {
            Some(idx) => {
                if idx >= self.open_windows.len() - 1 {
                    Some(0)
                } else {
                    Some(idx + 1)
                }
            }
            None if self.open_windows.len() > 0 => Some(0),
            None => None,
        }
    }

    fn _prev_window_idx(&mut self) -> Option<usize> {
        match self.active_window_idx {
            Some(idx) => {
                if idx == 0 {
                    Some(self.open_windows.len() - 1)
                } else {
                    Some(idx - 1)
                }
            }
            None if self.open_windows.len() > 0 => Some(0),
            None => None,
        }
    }

    fn next_window_idx(&mut self) -> Option<usize> {
        match self.layout {
            Layout::TileHorizontal(_) => self._next_window_idx(),
            _ => self._prev_window_idx(),
        }
    }

    fn prev_window_idx(&mut self) -> Option<usize> {
        match self.layout {
            Layout::TileHorizontal(_) => self._prev_window_idx(),
            _ => self._next_window_idx(),
        }
    }

    fn next_window(&mut self) -> Result<()> {
        self.active_window_idx = self.next_window_idx();
        self.activate_active_window()
    }

    fn prev_window(&mut self) -> Result<()> {
        self.active_window_idx = self.prev_window_idx();
        self.activate_active_window()
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

    fn set_active_window_full(&self) -> Result<()> {
        if let Some(window) = self.get_active_window()? {
            let display = window.display()?;
            window.set_frame(display.bounds())?;
        }
        Ok(())
    }

    fn set_active_window_left(&self) -> Result<()> {
        if let Some(window) = self.get_active_window()? {
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

    fn set_active_window_right(&self) -> Result<()> {
        if let Some(window) = self.get_active_window()? {
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
        if let Some(window) = self.get_active_window()? {
            window.set_minimized(true)?;
            let idx = self.active_window_idx.unwrap();
            let w = self.open_windows.remove(idx);
            self.minimized_windows.push(w);
            self.active_window_idx = if self.open_windows.len() == 0 {
                None
            } else {
                Some(usize::min(idx, self.open_windows.len() - 1))
            };
            Ok(())
        } else {
            Ok(())
        }
    }

    fn unminimize_window(&mut self) -> Result<()> {
        if let Some(window) = self.minimized_windows.pop() {
            window.set_minimized(false)?;
            self.open_windows.insert(0, window);
            self.active_window_idx = Some(0);
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        eprintln!("set_layout: {:?}", self.layout);
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
        self.layout.apply(&self.open_windows)
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

    pub fn do_action(&mut self, action: &Action) -> Result<()> {
        use Action::*;
        match action {
            RefreshWindowList => self.refresh_window_list(),
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
                self.relayout()
            }
            LayoutCascade => {
                self.set_layout_cascade();
                self.relayout()
            }
            LayoutTiling => {
                self.set_layout_tile_horizontal();
                self.relayout()
            }
            WindowFull => self.set_active_window_full(),
            WindowLeftHalf => self.set_active_window_left(),
            WindowRightHalf => self.set_active_window_right(),
            WindowMinimize => {
                self.minimize_active_window()?;
                self.activate_active_window()?;
                self.relayout()
            }
            WindowRestore => {
                self.unminimize_window()?;
                self.activate_active_window()?;
                self.relayout()
            }
            NextWindow => self.next_window(),
            PrevWindow => self.prev_window(),
            SwapNextWindow => {
                self.swap_window_next();
                self.relayout()
            }
            SwapPrevWindow => {
                self.swap_window_prev();
                self.relayout()
            }
            IncrPrimaryColWidth => {
                self.incr_primary_column_width();
                self.relayout()
            }
            DecrPrimaryColWidth => {
                self.decr_primary_column_width();
                self.relayout()
            }
            IncrPrimaryColWindows => {
                self.incr_primary_column_max_windows();
                self.relayout()
            }
            DecrPrimaryColWindows => {
                self.decr_primary_column_max_windows();
                self.relayout()
            }
        }
    }
}
