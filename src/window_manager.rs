use std::ffi::c_void;

use accessibility::{AXUIElement, AXUIElementAttributes};
use accessibility_sys::kAXWindowRole;
use anyhow::{anyhow, Result};
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
    drag_window::DragWindow,
    window::{Window, WindowWrapper},
    CGErrorWrapper,
};

pub use crate::layout::Layout;

#[derive(Debug, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

fn get_all_windows() -> Result<Vec<WindowWrapper<AXUIElement>>> {
    let window_list: CFArray<*const c_void> = CGDisplay::window_list_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        None,
    )
    .ok_or(anyhow!("no window_list_info"))?;

    let window_pids = window_list
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
        });

    let mut window_pids_deduped = vec![];
    for pid in window_pids {
        if !window_pids_deduped.contains(&pid) {
            window_pids_deduped.push(pid);
        }
    }

    let apps = window_pids_deduped
        .iter()
        .map(|pid| AXUIElement::application(*pid as i32))
        .collect::<Vec<_>>();

    let mut res = vec![];
    for app in apps {
        match app.windows() {
            Ok(windows) => {
                for w in windows.iter() {
                    if w.role()? == kAXWindowRole {
                        let w = WindowWrapper(w.clone());
                        w.debug_attributes()?;
                        res.push(w);
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

    eprintln!("window list: {:?}", res);

    Ok(res)
}

#[derive(Debug)]
pub struct WindowManager {
    drag_window: Option<DragWindow>,
    mode: Mode,
    layout: Layout,
    active_window_idx: Option<usize>,
    windows: Vec<WindowWrapper<AXUIElement>>,
    max_primary_column_windows: i32,
}

impl WindowManager {
    pub fn new() -> Self {
        WindowManager {
            drag_window: None,
            mode: Mode::Normal,
            layout: Layout::Floating,
            active_window_idx: None,
            windows: vec![],
            max_primary_column_windows: 1,
        }
    }

    pub fn refresh_window_list(&mut self) -> Result<()> {
        self.windows = get_all_windows()?;
        self.active_window_idx = self
            .windows
            .iter()
            .position(|w| w.frontmost_and_main().unwrap_or(false));
        if self.active_window_idx.is_none() {
            eprintln!("No active window!");
        }
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

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Normal => Mode::Insert,
            Mode::Insert => Mode::Normal,
        };
        println!("Entered {:?} mode", self.mode);
    }

    pub fn exit_normal_mode(&mut self) {
        if self.mode != Mode::Insert {
            self.mode = Mode::Insert;
            println!("Entered {:?} mode", self.mode);
        }
    }

    fn get_active_window(&self) -> Result<Option<&WindowWrapper<AXUIElement>>> {
        match self.active_window_idx {
            None => Ok(None),
            Some(idx) => {
                let window = self
                    .windows
                    .get(idx)
                    .ok_or(accessibility::Error::NotFound)?;
                Ok(Some(window))
            }
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
        self.active_window_idx.map(|idx| {
            if idx >= self.windows.len() - 1 {
                0
            } else {
                idx + 1
            }
        })
    }

    fn _prev_window_idx(&mut self) -> Option<usize> {
        self.active_window_idx.map(|idx| {
            if idx == 0 {
                self.windows.len() - 1
            } else {
                idx - 1
            }
        })
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

    pub fn next_window(&mut self) -> Result<()> {
        self.active_window_idx = self.next_window_idx();
        self.activate_active_window()
    }

    pub fn prev_window(&mut self) -> Result<()> {
        self.active_window_idx = self.prev_window_idx();
        self.activate_active_window()
    }

    pub fn swap_window_prev(&mut self) {
        match (self.active_window_idx, self.prev_window_idx()) {
            (Some(idx), Some(prev_idx)) => {
                self.windows.swap(idx, prev_idx);
                self.active_window_idx = Some(prev_idx);
            }
            _ => (),
        }
    }

    pub fn swap_window_next(&mut self) {
        match (self.active_window_idx, self.next_window_idx()) {
            (Some(idx), Some(next_idx)) => {
                self.windows.swap(idx, next_idx);
                self.active_window_idx = Some(next_idx);
            }
            _ => (),
        }
    }

    pub fn set_active_window_full(&self) -> Result<()> {
        if let Some(window) = self.get_active_window()? {
            let display = window.display()?;
            window.set_frame(display.bounds())?;
        }
        Ok(())
    }

    pub fn set_active_window_left(&self) -> Result<()> {
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

    pub fn set_active_window_right(&self) -> Result<()> {
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

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    fn set_layout(&mut self, layout: Layout) {
        self.layout = layout;
        eprintln!("set_layout: {:?}", self.layout);
    }

    pub fn set_layout_floating(&mut self) {
        self.set_layout(Layout::floating())
    }
    pub fn set_layout_cascade(&mut self) {
        self.set_layout(Layout::cascade())
    }
    pub fn set_layout_tile_horizontal(&mut self) {
        self.set_layout(Layout::tile_horizontal(self.max_primary_column_windows))
    }

    pub fn relayout(&self) -> Result<()> {
        self.layout.apply(&self.windows)
    }

    pub fn incr_max_primary_column_windows(&mut self) {
        self.max_primary_column_windows = i32::min(
            self.max_primary_column_windows + 1,
            self.windows.len() as i32,
        );
        self.set_layout(Layout::tile_horizontal(self.max_primary_column_windows));
    }

    pub fn decr_max_primary_column_windows(&mut self) {
        self.max_primary_column_windows = i32::max(self.max_primary_column_windows - 1, 1);
        self.set_layout(Layout::tile_horizontal(self.max_primary_column_windows));
    }
}
