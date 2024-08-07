use std::cell::RefCell;
use std::ffi::c_void;

use accessibility::AXUIElement;
use awesome_rs::{Action, DragWindow, WindowManager, HELP_TEXT};
use cocoa::appkit::{NSApp, NSApplication};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapCallbackResult, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventType,
};

const AWESOME_ENABLE_DRAG_WINDOW: bool = false;

// <ALT>
fn awesome_normal_mode_drag_window_flags() -> CGEventFlags {
    CGEventFlags::CGEventFlagAlternate
}

fn main() {
    let wm = WindowManager::new();
    let state: RefCell<WindowManager> = RefCell::new(wm);

    let event_tap = {
        use CGEventType::*;
        CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![MouseMoved, FlagsChanged, KeyDown],
            mk_event_tap_callback(&state),
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

    println!("{}", HELP_TEXT);

    unsafe {
        // let _pool = NSAutoreleasePool::new(nil);
        let app = NSApp();
        app.run();
    }
}

fn mk_event_tap_callback<'a>(
    state: &'a RefCell<WindowManager>,
) -> impl Fn(*const c_void, CGEventType, &CGEvent) -> CGEventTapCallbackResult + 'a {
    use CGEventType::*;
    |_, event_type, event| -> CGEventTapCallbackResult {
        let mut s = state.borrow_mut();
        match event_type {
            MouseMoved => {
                if let Some(dw) = s.drag_window() {
                    dw.set_position_around(&event.location()).unwrap()
                }
            }
            FlagsChanged if AWESOME_ENABLE_DRAG_WINDOW => {
                if event
                    .get_flags()
                    .contains(awesome_normal_mode_drag_window_flags())
                    && s.is_normal_mode()
                {
                    let ws = DragWindow::at_mouse_location().unwrap_or_else(|e| {
                        eprintln!("While getting window at mouse location: {}", e);
                        None
                    });
                    s.set_drag_window(ws);
                    if let Some(drag_window) = s.drag_window() {
                        drag_window
                            .activate_window()
                            .unwrap_or_else(|e| eprintln!("While activating drag window: {:?}", e))
                    }
                } else {
                    s.set_drag_window(None);
                }
            }
            _ => (),
        };
        match Action::of_cg_event(&event, &s.mode(), s.layout()) {
            Some(action) => {
                s.do_action(&action)
                    .unwrap_or_else(|e| eprintln!("While performing {:?}: {:?}", action, e));
                CGEventTapCallbackResult::Drop
            }
            None => CGEventTapCallbackResult::Keep,
        }
    }
}
