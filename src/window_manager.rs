use std::{collections::HashSet, ffi::c_void};

use accessibility::{AXUIElement, AXUIElementAttributes};
use anyhow::Result;
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

#[derive(Debug, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

fn get_all_windows() -> Vec<AXUIElement> {
    let window_list: CFArray<*const c_void> = CGDisplay::window_list_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        None,
    )
    .unwrap();

    let window_pids: HashSet<i64> = window_list
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
            let k: CFString = unsafe { CFString::wrap_under_create_rule(kCGWindowOwnerPID) };
            let pid = d.get(k.to_void());
            let pid = unsafe { CFNumber::from_void(*pid) };
            pid.to_i64()
        })
        .collect();

    let apps = window_pids
        .iter()
        .map(|pid| AXUIElement::application(*pid as i32))
        .collect::<Vec<_>>();

    let mut res = vec![];
    for app in apps {
        for w in app.windows().unwrap().iter() {
            res.push(w.clone());
        }
    }

    res
}

#[derive(Debug)]
pub struct WindowManager {
    drag_window: Option<DragWindow>,
    mode: Mode,
    active_window_idx: usize,
    windows: Vec<AXUIElement>,
}

impl WindowManager {
    pub fn new() -> Self {
        let windows = get_all_windows();
        WindowManager {
            drag_window: None,
            mode: Mode::Insert,
            active_window_idx: 0,
            windows,
        }
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

    fn get_active_window(&self) -> Result<WindowWrapper<&AXUIElement>> {
        let window = self
            .windows
            .get(self.active_window_idx)
            .ok_or(accessibility::Error::NotFound)?;
        Ok(WindowWrapper(window))
    }

    fn activate_active_window(&self) -> Result<()> {
        let w = self.get_active_window()?;
        w.activate()
    }

    fn incr_active_window(&mut self) {
        if self.active_window_idx >= self.windows.len() - 1 {
            self.active_window_idx = 0;
        } else {
            self.active_window_idx += 1;
        }
    }

    fn decr_active_window(&mut self) {
        if self.active_window_idx == 0 {
            self.active_window_idx = self.windows.len() - 1;
        } else {
            self.active_window_idx -= 1;
        }
    }

    pub fn next_window(&mut self) -> Result<()> {
        self.incr_active_window();
        self.activate_active_window()
    }

    pub fn prev_window(&mut self) -> Result<()> {
        self.decr_active_window();
        self.activate_active_window()
    }

    pub fn set_active_window_full(&self) -> Result<()> {
        let window = self.get_active_window()?;
        let display = window.display()?;
        window.set_frame(display.bounds())?;
        Ok(())
    }

    pub fn set_active_window_left(&self) -> Result<()> {
        let window = self.get_active_window()?;
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
                ))
            } else {
                Ok(())
            }
        } else {
            window.set_frame(CGRect::new(
                &d.origin,
                &CGSize::new(d.size.width / 2., d.size.height),
            ))
        }
    }

    pub fn set_active_window_right(&self) -> Result<()> {
        let window = self.get_active_window()?;
        let d = window.display()?.bounds();
        let w = window.frame()?;
        if w.origin.x == d.origin.x + d.size.width / 2. && w.size.width == d.size.width / 2. {
            let pos = CGPoint::new(d.origin.x + d.size.width + 1.0, d.origin.y);
            let (displays, _) = CGDisplay::displays_with_point(pos, 1).map_err(CGErrorWrapper)?;
            if let Some(display_id) = displays.first() {
                let d = CGDisplay::new(*display_id).bounds();
                window.set_frame(CGRect::new(
                    &d.origin,
                    &CGSize::new(d.size.width / 2., d.size.height),
                ))
            } else {
                Ok(())
            }
        } else {
            window.set_frame(CGRect::new(
                &CGPoint::new(d.origin.x + d.size.width / 2., d.origin.y),
                &CGSize::new(d.size.width / 2., d.size.height),
            ))
        }
    }
}
