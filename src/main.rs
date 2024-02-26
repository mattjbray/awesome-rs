use std::cell::RefCell;

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use accessibility_sys::kAXApplicationRole;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::CFString;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};

use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

struct WindowState {
    window: AXUIElement,
    mouse_offset: CGPoint,
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

    fn position_around(&self, point: &CGPoint) {
        let x = point.x - self.mouse_offset.x;
        let y = point.y - self.mouse_offset.y;

        let position = CGPoint::new(x, y);

        self.window
            .set_attribute(
                &AXAttribute::position(),
                AXValue::from_CGPoint(position).unwrap(),
            )
            .unwrap();
    }

    fn activate(&self) {
        get_application(&self.window).unwrap()
            .set_attribute(&AXAttribute::frontmost(), true)
            .unwrap();
        self.window.set_main(true).unwrap();
    }
}

fn main() {
    let system_wide_element = AXUIElement::system_wide();

    let state: RefCell<Option<WindowState>> = RefCell::new(None);

    let event_tap = {
        use CGEventType::*;
        CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![MouseMoved, FlagsChanged],
            |_, event_type, event| {
                match event_type {
                    MouseMoved => {
                        if let Some(state) = state.borrow().as_ref() {
                            state.position_around(&event.location())
                        }
                    }

                    FlagsChanged => {
                        let mut s = state.borrow_mut();
                        if event.get_flags().contains(CGEventFlags::CGEventFlagCommand) {
                            *s = WindowState::from_mouse_location(&system_wide_element);
                            if let Some(window_state) = s.as_ref() {
                                window_state.activate()
                            }
                        } else {
                            *s = None;
                        }
                    }
                    _ => (),
                };
                None
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
