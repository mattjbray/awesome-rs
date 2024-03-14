use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::c_void;

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use accessibility_sys::kAXApplicationRole;
use core_foundation::array::CFArray;
use core_foundation::base::{FromVoid, ItemRef, TCFType, ToVoid};
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::CFString;
use core_graphics::display::{
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, CFDictionary, CGDisplay,
    CGSize,
};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapCallbackResult, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventType, EventField,
};

use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use core_graphics::window::{kCGWindowLayer, kCGWindowOwnerPID};

// <CTL> + <ALT>
fn awesome_normal_mode_flags() -> CGEventFlags {
    CGEventFlags::CGEventFlagAlternate | CGEventFlags::CGEventFlagControl
}

// <ALT>
fn awesome_normal_mode_drag_window_flags() -> CGEventFlags {
    CGEventFlags::CGEventFlagAlternate
}

const AWESOME_NORMAL_MODE_WINDOW_LEFT_KEY: i64 = 4; // h
const AWESOME_NORMAL_MODE_WINDOW_RIGHT_KEY: i64 = 37; // l
const AWESOME_NORMAL_MODE_WINDOW_FULL_KEY: i64 = 36; // <ENTER>
const AWESOME_NORMAL_MODE_NEXT_WINDOW_KEY: i64 = 38; // j
const AWESOME_NORMAL_MODE_PREV_WINDOW_KEY: i64 = 40; // k

#[derive(Debug)]
struct Window(AXUIElement);

#[derive(Debug)]
struct WindowState {
    window: Window,
    mouse_offset: CGPoint,
}

#[derive(Debug, PartialEq)]
enum Mode {
    Normal,
    Insert,
}

#[derive(Debug)]
struct State {
    window_state: Option<WindowState>,
    mode: Mode,
    active_window: usize,
    app_windows: Vec<CFArray<AXUIElement>>,
    window_idxs: Vec<(usize, isize)>,
}

impl State {
    fn new(app_windows: Vec<CFArray<AXUIElement>>) -> Self {
        let window_idxs = app_windows
            .iter()
            .enumerate()
            .flat_map(|(i, arr)| (0..(arr.len())).into_iter().map(move |j| (i, j)))
            .collect();
        State {
            window_state: None,
            mode: Mode::Insert,
            active_window: 0,
            app_windows,
            window_idxs,
        }
    }

    fn activate_active_window(&self) -> Result<(), accessibility::Error> {
        let (i, j) = self
            .window_idxs
            .get(self.active_window)
            .ok_or(accessibility::Error::NotFound)?;
        let app_ws = self
            .app_windows
            .get(*i)
            .ok_or(accessibility::Error::NotFound)?;
        let w = app_ws.get(*j).ok_or(accessibility::Error::NotFound)?;
        window_activate(&w)
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

    fn next_window(&mut self) -> Result<(), accessibility::Error> {
        self.incr_active_window();
        self.activate_active_window()
    }

    fn prev_window(&mut self) -> Result<(), accessibility::Error> {
        self.decr_active_window();
        self.activate_active_window()
    }
}

fn get_application(element: &AXUIElement) -> Result<AXUIElement, accessibility::Error> {
    let role = element.role()?;
    if role == CFString::from_static_string(kAXApplicationRole) {
        Ok(element.clone())
    } else {
        let pid = element.pid()?;
        Ok(AXUIElement::application(pid))
    }
}

fn get_mouse_location() -> Result<CGPoint, ()> {
    CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|e| e.location())
}

impl Window {
    fn from_ui_element(element: AXUIElement) -> Result<Window, accessibility::Error> {
        let element_is_window = match element.role() {
            Ok(role) => role == CFString::from_static_string(accessibility_sys::kAXWindowRole),
            _ => false,
        };

        let window = if element_is_window {
            Ok(element)
        } else {
            element.window()
        }?;

        Ok(Self(window))
    }

    fn at_point(
        system_wide_element: &AXUIElement,
        point: &CGPoint,
    ) -> Result<Self, accessibility::Error> {
        let element = system_wide_element
            .element_at_position(point.x as f32, point.y as f32)
            .unwrap();

        Self::from_ui_element(element)
    }

    fn active() -> Result<Self, accessibility::Error> {
        let element = AXUIElement::system_wide().focused_uielement()?;
        Self::from_ui_element(element)
    }

    fn get_position(&self) -> Result<CGPoint, accessibility::Error> {
        let value = self.0.position()?;
        value.get_value()
    }

    fn set_position(&self, x: f64, y: f64) -> Result<(), accessibility::Error> {
        let position = CGPoint::new(x, y);
        self.0.set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(position).unwrap(),
        )?;
        println!(
            "set_position desired:{:?} result:{:?}",
            position,
            self.get_position()
        );
        Ok(())
    }

    fn get_size(&self) -> Result<CGSize, accessibility::Error> {
        let value = self.0.size()?;
        value.get_value()
    }

    fn set_size(&self, w: f64, h: f64) -> Result<(), accessibility::Error> {
        let size = CGSize::new(w, h);
        self.0
            .set_attribute(&AXAttribute::size(), AXValue::from_CGSize(size).unwrap())?;
        println!("set_size desired:{:?} result:{:?}", size, self.get_size());
        Ok(())
    }

    fn set_frame(&self, x: f64, y: f64, w: f64, h: f64) -> Result<(), accessibility::Error> {
        self.set_position(x, y)?;
        self.set_size(w, h)
    }

    /// Bring this window's application to front, and set this window as main.
    fn _activate(&self) -> Result<(), accessibility::Error> {
        window_activate(&self.0)
    }

    fn get_display(&self) -> Result<CGDisplay, accessibility::Error> {
        let position = self.get_position()?;
        let (displays, _) = CGDisplay::displays_with_point(position, 1).unwrap();
        let display_id = displays.first().ok_or(accessibility::Error::NotFound)?;
        let display = CGDisplay::new(*display_id);
        Ok(display)
    }
}

fn window_activate(window: &AXUIElement) -> Result<(), accessibility::Error> {
    let app = get_application(&window)?;
    app.set_attribute(&AXAttribute::frontmost(), true)?;
    window.set_main(true)
}

fn display_bounds(display: &CGDisplay) -> (f64, f64, f64, f64) {
    let b = display.bounds();
    (b.origin.x, b.origin.y, b.size.width, b.size.height)
}

impl WindowState {
    fn new(window: Window, mouse_offset: CGPoint) -> Self {
        Self {
            window,
            mouse_offset,
        }
    }

    fn at_mouse_location() -> Option<Self> {
        let mouse_location = get_mouse_location().unwrap();
        let window = Window::at_point(&AXUIElement::system_wide(), &mouse_location);

        window
            .map(|window| {
                let window_pos: CGPoint = window.get_position().unwrap();
                let mouse_offset = CGPoint::new(
                    mouse_location.x - window_pos.x,
                    mouse_location.y - window_pos.y,
                );
                Self::new(window, mouse_offset)
            })
            .ok()
    }

    fn set_position_around(&self, point: &CGPoint) -> Result<(), accessibility::Error> {
        let x = point.x - self.mouse_offset.x;
        let y = point.y - self.mouse_offset.y;

        self.window.set_position(x, y)
    }
}

fn main() {
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
    println!("window pids: {:?}", window_pids);
    let apps = window_pids
        .iter()
        .map(|pid| AXUIElement::application(*pid as i32))
        .collect::<Vec<_>>();
    println!("apps: {:?}", apps);
    let app_windows: Vec<_> = apps.iter().map(|a| a.windows().unwrap()).collect();
    println!("app windows: {:?}", app_windows);

    let state: RefCell<State> = RefCell::new(State::new(app_windows));

    let event_tap = {
        use CGEventType::*;
        CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![MouseMoved, FlagsChanged, KeyDown],
            |_, event_type, event| {
                match event_type {
                    MouseMoved => {
                        if let Some(state) = state.borrow().window_state.as_ref() {
                            state.set_position_around(&event.location()).unwrap()
                        }
                        CGEventTapCallbackResult::Keep
                    }

                    FlagsChanged => {
                        // println!("FlagsChanged {:?}", event.get_flags());
                        let mut s = state.borrow_mut();
                        if event.get_flags().contains(awesome_normal_mode_flags()) {
                            s.mode = match s.mode {
                                Mode::Normal => Mode::Insert,
                                Mode::Insert => Mode::Normal,
                            };
                            println!("Entered {:?} mode", s.mode);
                        } else if event
                            .get_flags()
                            .contains(awesome_normal_mode_drag_window_flags())
                            && s.mode == Mode::Normal
                        {
                            s.window_state = WindowState::at_mouse_location();
                            // if let Some(window_state) = s.window_state.as_ref() {
                            //     window_state.window.activate().unwrap()
                            // }
                        } else {
                            s.window_state = None;
                        }
                        CGEventTapCallbackResult::Keep
                    }
                    KeyDown => {
                        let keycode =
                            event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        println!("KeyDown {}", keycode);
                        let mut s = state.borrow_mut();
                        match (&s.mode, keycode) {
                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_FULL_KEY) => {
                                let window = Window::active();
                                if let Ok(window) = window.as_ref() {
                                    let (x, y, w, h) =
                                        display_bounds(&window.get_display().unwrap());
                                    window.set_frame(x, y, w, h).unwrap();
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_LEFT_KEY) => {
                                let window = Window::active();
                                if let Ok(window) = window.as_ref() {
                                    let (x, y, w, h) =
                                        display_bounds(&window.get_display().unwrap());
                                    let position = window.get_position().unwrap();
                                    let size = window.get_size().unwrap();
                                    if x > 0. && position.x == x && size.width == w / 2. {
                                        let pos = CGPoint::new(x - 1.0, y);
                                        let (displays, _) =
                                            CGDisplay::displays_with_point(pos, 1).unwrap();
                                        if let Some(display_id) = displays.first() {
                                            let display = CGDisplay::new(*display_id);
                                            let (x, y, w, h) = display_bounds(&display);
                                            window.set_frame(x + w / 2., y, w / 2., h).unwrap();
                                        }
                                    } else {
                                        window.set_frame(x, y, w / 2., h).unwrap();
                                    }
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_RIGHT_KEY) => {
                                let window = Window::active();
                                if let Ok(window) = window.as_ref() {
                                    let (x, y, w, h) =
                                        display_bounds(&window.get_display().unwrap());
                                    let position = window.get_position().unwrap();
                                    let size = window.get_size().unwrap();
                                    if position.x == x + w / 2. && size.width == w / 2. {
                                        let pos = CGPoint::new(x + w + 1.0, y);
                                        let (displays, _) =
                                            CGDisplay::displays_with_point(pos, 1).unwrap();
                                        if let Some(display_id) = displays.first() {
                                            let display = CGDisplay::new(*display_id);
                                            let (x, y, w, h) = display_bounds(&display);
                                            window.set_frame(x, y, w / 2., h).unwrap();
                                        }
                                    } else {
                                        window.set_frame(x + w / 2., y, w / 2., h).unwrap();
                                    }
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_NEXT_WINDOW_KEY) => {
                                s.next_window().unwrap();
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_PREV_WINDOW_KEY) => {
                                s.prev_window().unwrap();
                                CGEventTapCallbackResult::Drop
                            }

                            _ => {
                                // Enter Insert mode on any other key
                                if s.mode != Mode::Insert {
                                    s.mode = Mode::Insert;
                                    println!("Entered {:?} mode", s.mode);
                                }
                                CGEventTapCallbackResult::Keep
                            }
                        }
                    }
                    _ => CGEventTapCallbackResult::Keep,
                }
            },
        )
        .unwrap()
    };

    let current = CFRunLoop::get_current();
    let loop_source = event_tap.mach_port.create_runloop_source(0).unwrap();
    unsafe {
        current.add_source(&loop_source, kCFRunLoopCommonModes);
    }
    event_tap.enable();

    println!(
        "Starting app. Trusted: {}",
        AXUIElement::application_is_trusted()
    );

    CFRunLoop::run_current();
}
