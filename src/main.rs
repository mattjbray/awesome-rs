use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::display::CGSize;
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
};

use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

fn position_window_around(window: &AXUIElement, point: &CGPoint) {
    let size: CGSize = window.size().unwrap().get_value().unwrap();
    let x = point.x - size.width / 2.;
    let y = point.y - size.height / 2.;

    let position = CGPoint::new(x, y);

    window
        .set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(position).unwrap(),
        )
        .unwrap();
}

fn main() {
    let mouse_location = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|e| e.location())
        .unwrap();

    let system_wide_element = AXUIElement::system_wide();

    let element = system_wide_element
        .element_at_position(mouse_location.x as f32, mouse_location.y as f32)
        .unwrap();

    let window = element.window().unwrap();

    position_window_around(&window, &CGPoint::new(mouse_location.x, mouse_location.y));

    let event_tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::MouseMoved],
        |_, _, event| {
            println!("{:?}", event.location());
            position_window_around(&window, &event.location());
            None
        },
    )
    .unwrap();

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
