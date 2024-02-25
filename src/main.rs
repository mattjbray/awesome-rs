use std::cell::RefCell;

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use cocoa::appkit::{NSApplicationActivationOptions, NSRunningApplication};
use cocoa::base::nil;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
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

impl WindowState {
    fn new(window: AXUIElement, mouse_offset: CGPoint) -> Self {
        Self {
            window,
            mouse_offset,
        }
    }
}

fn get_window_under_mouse(system_wide_element: &AXUIElement) -> Option<WindowState> {
    let mouse_location = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|e| e.location())
        .unwrap();

    let element = system_wide_element
        .element_at_position(mouse_location.x as f32, mouse_location.y as f32)
        .unwrap();

    element
        .window()
        .map(|window| {
            let window_pos: CGPoint = window.position().unwrap().get_value().unwrap();
            let mouse_offset = CGPoint::new(
                mouse_location.x - window_pos.x,
                mouse_location.y - window_pos.y,
            );
            WindowState::new(window, mouse_offset)
        })
        .ok()
}

fn position_window_around(s: &WindowState, point: &CGPoint) {
    let x = point.x - s.mouse_offset.x;
    let y = point.y - s.mouse_offset.y;

    let position = CGPoint::new(x, y);

    s.window
        .set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(position).unwrap(),
        )
        .unwrap();
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
                    MouseMoved => match state.borrow().as_ref() {
                        Some(state) => position_window_around(&state, &event.location()),
                        None => (),
                    },

                    FlagsChanged => {
                        let mut s = state.borrow_mut();
                        if event.get_flags().contains(CGEventFlags::CGEventFlagCommand) {
                            *s = get_window_under_mouse(&system_wide_element);
                            if let Some(window_state) = s.as_ref() {
                                let pid = window_state.window.pid().unwrap();
                                unsafe {
                                    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(nil, pid);
                                    app.activateWithOptions_(NSApplicationActivationOptions::NSApplicationActivateAllWindows);
                                }
                                window_state.window.set_main(true).unwrap();
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
