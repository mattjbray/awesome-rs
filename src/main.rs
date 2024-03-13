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

#[derive(Debug)]
struct WindowState {
    window: AXUIElement,
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

impl WindowState {
    fn new(window: AXUIElement, mouse_offset: CGPoint) -> Self {
        Self {
            window,
            mouse_offset,
        }
    }

    fn from_mouse_location(system_wide_element: &AXUIElement) -> Option<Self> {
        let mouse_location = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .and_then(CGEvent::new)
            .map(|e| e.location())
            .unwrap();

        let element = system_wide_element
            .element_at_position(mouse_location.x as f32, mouse_location.y as f32)
            .unwrap();

        let element_is_window = match element.role() {
            Ok(role) => role == CFString::from_static_string(accessibility_sys::kAXWindowRole),
            _ => false,
        };

        let window = if element_is_window {
            Ok(element)
        } else {
            element.window()
        };

        window
            .map(|window| {
                let window_pos: CGPoint = window.position().unwrap().get_value().unwrap();
                let mouse_offset = CGPoint::new(
                    mouse_location.x - window_pos.x,
                    mouse_location.y - window_pos.y,
                );
                Self::new(window, mouse_offset)
            })
            .ok()
    }

    fn position(&self, x: f64, y: f64) {
        let position = CGPoint::new(x, y);

        self.window
            .set_attribute(
                &AXAttribute::position(),
                AXValue::from_CGPoint(position).unwrap(),
            )
            .unwrap();
    }

    fn position_around(&self, point: &CGPoint) {
        let x = point.x - self.mouse_offset.x;
        let y = point.y - self.mouse_offset.y;

        self.position(x, y);
    }

    fn resize(&self, w: f64, h: f64) {
        let size = CGSize::new(w, h);
        self.window
            .set_attribute(&AXAttribute::size(), AXValue::from_CGSize(size).unwrap())
            .unwrap();
    }

    fn activate(&self) {
        get_application(&self.window)
            .unwrap()
            .set_attribute(&AXAttribute::frontmost(), true)
            .unwrap();
        self.window.set_main(true).unwrap();
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
                            state.position_around(&event.location())
                        }
                        CGEventTapCallbackResult::Keep
                    }

                    FlagsChanged => {
                        let mut s = state.borrow_mut();
                        if event.get_flags().contains(CGEventFlags::CGEventFlagCommand) && s.mode == Mode::Normal {
                            s.window_state = WindowState::from_mouse_location(&system_wide_element);
                            if let Some(window_state) = s.window_state.as_ref() {
                                window_state.activate()
                            }
                        } else if event
                            .get_flags()
                            .contains(CGEventFlags::CGEventFlagAlternate)
                            && event.get_flags().contains(CGEventFlags::CGEventFlagControl)
                        {
                            s.mode = match s.mode {
                                Mode::Normal => Mode::Insert,
                                Mode::Insert => Mode::Normal,
                            };
                            println!("Entered {:?} mode", s.mode);
                        } else {
                            s.window_state = None;
                        }
                        CGEventTapCallbackResult::Keep
                    }
                    KeyDown => {
                        let keycode =
                            event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                        println!("{}", keycode);
                        match (&state.borrow().mode, keycode) {
                            (Mode::Normal, 4) => {
                                // h
                                let ws = WindowState::from_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.activate();
                                    window_state.position(0., 0.);
                                    window_state.resize(w as f64 / 2., h as f64);
                                }
                                CGEventTapCallbackResult::Drop
                            }

                            (Mode::Normal, 36) => {
                                // <ENTER>
                                let ws = WindowState::from_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.activate();
                                    window_state.position(0., 0.);
                                    window_state.resize(w as f64, h as f64);
                                }
                                CGEventTapCallbackResult::Drop
                            }
                            (Mode::Normal, 37) => {
                                // l
                                let ws = WindowState::from_mouse_location(&system_wide_element);
                                if let Some(window_state) = ws.as_ref() {
                                    window_state.activate();
                                    window_state.position(w as f64 / 2., 0.);
                                    window_state.resize(w as f64 / 2., h as f64);
                                }
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
