use std::cell::RefCell;
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

const AWESOME_NORMAL_MODE_INSERT_MODE_KEY:i64 = 53; // <ESC>
const AWESOME_NORMAL_MODE_WINDOW_LEFT_KEY:i64 = 4; // h
const AWESOME_NORMAL_MODE_WINDOW_RIGHT_KEY:i64 = 37; // l
const AWESOME_NORMAL_MODE_WINDOW_FULL_KEY:i64 = 36; // <ENTER>

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
}

impl State {
    fn new() -> Self {
        State {
            window_state: None,
            mode: Mode::Insert,
        }
    }
}

fn get_application(element: &AXUIElement) -> Result<AXUIElement, accessibility::Error> {
    let role = element.role()?;
    if role == CFString::from_static_string(kAXApplicationRole) {
        Ok(element.clone())
    } else {
        let parent = element.parent()?;
        get_application(&parent)
    }
}

fn get_mouse_location() -> Result<CGPoint, ()> {
    CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|e| e.location())
}

impl Window {
    fn at_point(
        system_wide_element: &AXUIElement,
        point: &CGPoint,
    ) -> Result<Self, accessibility::Error> {
        let element = system_wide_element
            .element_at_position(point.x as f32, point.y as f32)
            .unwrap();

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

    fn get_position(&self) -> Result<CGPoint, accessibility::Error> {
        let value = self.0.position()?;
        value.get_value()
    }

    fn set_position(&self, x: f64, y: f64) -> Result<(), accessibility::Error> {
        self.0.set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(CGPoint::new(x, y)).unwrap(),
        )
    }

    fn set_size(&self, w: f64, h: f64) -> Result<(), accessibility::Error> {
        let size = CGSize::new(w, h);
        self.0
            .set_attribute(&AXAttribute::size(), AXValue::from_CGSize(size).unwrap())
    }

    /// Bring this window's application to front, and set this window as main.
    fn activate(&self) -> Result<(), accessibility::Error> {
        let app = get_application(&self.0)?;
        app.set_attribute(&AXAttribute::frontmost(), true)?;
        self.0.set_main(true)
    }
}

impl WindowState {
    fn new(window: Window, mouse_offset: CGPoint) -> Self {
        Self {
            window,
            mouse_offset,
        }
    }

    fn at_mouse_location(system_wide_element: &AXUIElement) -> Option<Self> {
        let mouse_location = get_mouse_location().unwrap();
        let window = Window::at_point(system_wide_element, &mouse_location);

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
    let system_wide_element = AXUIElement::system_wide();

    let d = CGDisplay::main();
    let w = d.pixels_wide();
    let h = d.pixels_high();
    println!("w:{} h:{}", w, h);

    let window_list: CFArray<*const c_void> = CGDisplay::window_list_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        None,
    )
    .unwrap();
    println!(
        "{:?}",
        window_list
            .iter()
            .map(|w| unsafe { CFDictionary::from_void(*w) })
            .filter(|d: &ItemRef<CFDictionary>| {
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
            .collect::<Vec<_>>()
    );

    let state: RefCell<State> = RefCell::new(State::new());

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
                            s.window_state = WindowState::at_mouse_location(&system_wide_element);
                            if let Some(window_state) = s.window_state.as_ref() {
                                window_state.window.activate().unwrap()
                            }
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
                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_LEFT_KEY) => {
                                let ws = WindowState::at_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.window.activate().unwrap();
                                    window_state.window.set_position(0., 0.).unwrap();
                                    window_state
                                        .window
                                        .set_size(w as f64 / 2., h as f64)
                                        .unwrap();
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_FULL_KEY) => {
                                let ws = WindowState::at_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.window.activate().unwrap();
                                    window_state.window.set_position(0., 0.).unwrap();
                                    window_state.window.set_size(w as f64, h as f64).unwrap();
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_WINDOW_RIGHT_KEY) => {
                                let ws = WindowState::at_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.window.activate().unwrap();
                                    window_state.window.set_position(w as f64 / 2., 0.).unwrap();
                                    window_state
                                        .window
                                        .set_size(w as f64 / 2., h as f64)
                                        .unwrap();
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, AWESOME_NORMAL_MODE_INSERT_MODE_KEY) => {
                                s.mode = Mode::Insert;
                                println!("Entered {:?} mode", s.mode);
                                CGEventTapCallbackResult::Drop
                            }
                            _ => CGEventTapCallbackResult::Keep,
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
