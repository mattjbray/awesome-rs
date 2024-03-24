use accessibility::AXUIElement;
use anyhow::{anyhow, Result};
use core_graphics::{
    event::CGEvent,
    event_source::{CGEventSource, CGEventSourceStateID},
    geometry::CGPoint,
};

use crate::window::{Window, WindowWrapper};

#[derive(Debug)]
pub struct DragWindow {
    window: WindowWrapper<AXUIElement>,
    mouse_offset: CGPoint,
}

fn get_mouse_location() -> Result<CGPoint> {
    let event_source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
        .map_err(|()| anyhow!("Failed to create CGEventSource"))?;
    let event = CGEvent::new(event_source).map_err(|()| anyhow!("Failed to create GCEvent"))?;
    Ok(event.location())
}

impl DragWindow {
    fn new(window: WindowWrapper<AXUIElement>, mouse_offset: CGPoint) -> Self {
        Self {
            window,
            mouse_offset,
        }
    }

    pub fn at_mouse_location() -> Result<Option<Self>> {
        let mouse_location = get_mouse_location()?;
        let window = WindowWrapper::at_point(&mouse_location)?;
        match window {
            None => Ok(None),
            Some(window) => {
                let window_pos: CGPoint = window.position()?;
                let mouse_offset = CGPoint::new(
                    mouse_location.x - window_pos.x,
                    mouse_location.y - window_pos.y,
                );
                Ok(Some(Self::new(window, mouse_offset)))
            }
        }
    }

    pub fn set_position_around(&self, point: &CGPoint) -> Result<()> {
        let x = point.x - self.mouse_offset.x;
        let y = point.y - self.mouse_offset.y;

        self.window.set_position(CGPoint::new(x, y))
    }

    pub fn activate_window(&self) -> Result<()> {
        self.window.activate()
    }
}
