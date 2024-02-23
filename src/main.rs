use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;

fn main() {
    let mouse_location = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .and_then(CGEvent::new)
        .map(|e| e.location())
        .unwrap();

    let system_wide_element = AXUIElement::system_wide();

    let element = system_wide_element
        .element_at_position(mouse_location.x as f32, mouse_location.y as f32)
        .unwrap();

    let pos = CGPoint::new(mouse_location.x, mouse_location.y);

    let window = element.window().unwrap();

    window
        .set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(pos).unwrap(),
        )
        .unwrap();

    println!(
        "Starting app. Trusted: {}",
        AXUIElement::application_is_trusted()
    );
}
