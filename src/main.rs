use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::c_void;

use accessibility::{AXUIElement, AXUIElementAttributes};
use awesome_rs::{DragWindow, WindowManager};
use core_foundation::{
    array::CFArray,
    base::{FromVoid, ItemRef, TCFType, ToVoid},
    number::CFNumber,
    runloop::{kCFRunLoopCommonModes, CFRunLoop},
    string::CFString,
};
use core_graphics::{
    display::{
        kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, CFDictionary,
        CGDisplay,
    },
    event::{
        CGEvent, CGEventFlags, CGEventTap, CGEventTapCallbackResult, CGEventTapLocation,
        CGEventTapOptions, CGEventTapPlacement, CGEventType, EventField,
    },
    window::{kCGWindowLayer, kCGWindowOwnerPID},
};

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

    let state: RefCell<WindowManager> = RefCell::new(WindowManager::new(app_windows));

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

    CFRunLoop::run_current();
}

fn mk_event_tap_callback(
    state: &RefCell<WindowManager>,
) -> impl Fn(*const c_void, CGEventType, &CGEvent) -> CGEventTapCallbackResult + '_ {
    use CGEventType::*;
    |_, event_type, event| {
        match event_type {
            MouseMoved => {
                if let Some(dw) = state.borrow().drag_window() {
                    dw.set_position_around(&event.location()).unwrap()
                }
                CGEventTapCallbackResult::Keep
            }

            FlagsChanged => {
                // println!("FlagsChanged {:?}", event.get_flags());
                let mut s = state.borrow_mut();
                if event.get_flags().contains(awesome_normal_mode_flags()) {
                    s.toggle_mode();
                } else if event
                    .get_flags()
                    .contains(awesome_normal_mode_drag_window_flags())
                    && s.is_normal_mode()
                {
                    let ws = DragWindow::at_mouse_location().unwrap_or_else(|e| {
                        eprintln!("While getting window at mouse location: {}", e);
                        None
                    });
                    s.set_drag_window(ws);
                    // if let Some(window_state) = s.window_state.as_ref() {
                    //     window_state.window.activate().unwrap()
                    // }
                } else {
                    s.set_drag_window(None);
                }
                CGEventTapCallbackResult::Keep
            }
            KeyDown => {
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                println!("KeyDown {}", keycode);
                let mut s = state.borrow_mut();
                match keycode {
                    AWESOME_NORMAL_MODE_WINDOW_FULL_KEY if s.is_normal_mode() => {
                        s.set_active_window_full()
                            .unwrap_or_else(|e| eprintln!("While setting window full: {}", e));
                        CGEventTapCallbackResult::Drop
                    }

                    AWESOME_NORMAL_MODE_WINDOW_LEFT_KEY if s.is_normal_mode() => {
                        s.set_active_window_left()
                            .unwrap_or_else(|e| eprintln!("While setting window left: {}", e));
                        CGEventTapCallbackResult::Drop
                    }

                    AWESOME_NORMAL_MODE_WINDOW_RIGHT_KEY if s.is_normal_mode() => {
                        s.set_active_window_right()
                            .unwrap_or_else(|e| eprintln!("While setting window right: {}", e));
                        CGEventTapCallbackResult::Drop
                    }

                    AWESOME_NORMAL_MODE_NEXT_WINDOW_KEY if s.is_normal_mode() => {
                        s.next_window()
                            .unwrap_or_else(|e| eprintln!("While switching to next window: {}", e));
                        CGEventTapCallbackResult::Drop
                    }

                    AWESOME_NORMAL_MODE_PREV_WINDOW_KEY if s.is_normal_mode() => {
                        s.prev_window()
                            .unwrap_or_else(|e| eprintln!("While switching to prev window: {}", e));
                        CGEventTapCallbackResult::Drop
                    }

                    _ => {
                        // Enter Insert mode on any other key
                        s.exit_normal_mode();
                        CGEventTapCallbackResult::Keep
                    }
                }
            }
            _ => CGEventTapCallbackResult::Keep,
        }
    }
}
