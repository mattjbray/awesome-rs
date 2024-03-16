use accessibility::AXUIElement;
use anyhow::Result;
use core_foundation::{array::CFArray, base::ItemRef};
use core_graphics::{
    display::CGDisplay,
    geometry::{CGPoint, CGRect, CGSize},
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

#[derive(Debug)]
pub struct WindowManager {
    drag_window: Option<DragWindow>,
    mode: Mode,
    active_window: usize,
    app_windows: Vec<CFArray<AXUIElement>>,
    window_idxs: Vec<(usize, isize)>,
}

impl WindowManager {
    pub fn new(app_windows: Vec<CFArray<AXUIElement>>) -> Self {
        let window_idxs = app_windows
            .iter()
            .enumerate()
            .flat_map(|(i, arr)| (0..(arr.len())).into_iter().map(move |j| (i, j)))
            .collect();
        WindowManager {
            drag_window: None,
            mode: Mode::Insert,
            active_window: 0,
            app_windows,
            window_idxs,
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

    fn get_active_window(&self) -> Result<WindowWrapper<ItemRef<'_, AXUIElement>>> {
        let (i, j) = self
            .window_idxs
            .get(self.active_window)
            .ok_or(accessibility::Error::NotFound)?;
        let app_ws = self
            .app_windows
            .get(*i)
            .ok_or(accessibility::Error::NotFound)?;
        let w = app_ws.get(*j).ok_or(accessibility::Error::NotFound)?;
        Ok(WindowWrapper(w))
    }

    fn activate_active_window(&self) -> Result<()> {
        let w = self.get_active_window()?;
        w.activate()
    }

    fn incr_active_window(&mut self) {
        if self.active_window >= self.window_idxs.len() - 1 {
            self.active_window = 0;
        } else {
            self.active_window += 1;
        }
    }

    fn decr_active_window(&mut self) {
        if self.active_window == 0 {
            self.active_window = self.window_idxs.len() - 1;
        } else {
            self.active_window -= 1;
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
