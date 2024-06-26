use std::{error::Error, fmt::Display, ops::Deref};

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use accessibility_sys::{kAXApplicationRole, kAXCloseButtonAttribute, kAXPressAction};
use anyhow::Result;
use cocoa::appkit::{NSApp, NSApplicationActivationOptions, NSRunningApplication};
use core_foundation::{
    base::{CFType, ItemRef, TCFType},
    boolean::CFBoolean,
    string::CFString,
};
use core_graphics::{
    base::CGError,
    display::{CGDisplay, CGPoint, CGRect, CGSize},
};

#[derive(Debug)]
pub struct CGErrorWrapper(pub CGError);

impl Display for CGErrorWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CGError {}", self.0)
    }
}

impl Error for CGErrorWrapper {}

#[allow(non_upper_case_globals)]
const kAXEnhancedUserInterfaceAttribute: &str = "AXEnhancedUserInterface";

pub trait Window {
    fn element(&self) -> &AXUIElement;

    fn application(&self) -> Result<AXUIElement> {
        let element = self.element();
        let role = element.role()?;
        if role == CFString::from_static_string(kAXApplicationRole) {
            Ok(element.clone())
        } else {
            let pid = element.pid()?;
            Ok(AXUIElement::application(pid))
        }
    }

    fn debug_attributes(&self) -> Result<()> {
        let w = self.element();
        eprintln!("{:?}", w);
        for attr in w.attribute_names()?.iter() {
            let val = w.attribute(&AXAttribute::new(&*attr));
            eprintln!("{:?}: {:?}", *attr, val);
        }
        Ok(())
    }

    /// Returns true if the other window has the same pid, title, position and
    /// size.
    /// Note: if this is insufficient, we could use the private
    /// _AXUIElementGetWindow API.
    /// See https://github.com/rxhanson/Rectangle/blob/main/Rectangle/Rectangle-Bridging-Header.h
    fn is_same_window(&self, other: &Self) -> Result<bool> {
        let pid = self.element().pid()?;
        let frame = self.frame()?;
        let title = self.element().title()?;

        if {
            let pid2 = other.element().pid()?;
            pid == pid2
        } && {
            let title2 = other.element().title()?;
            title == title2
        } && {
            let frame2 = other.frame()?;
            frame.origin.x == frame2.origin.x
                && frame.origin.y == frame2.origin.y
                && frame.size.width == frame2.size.width
                && frame.size.height == frame2.size.height
        } {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn position(&self) -> Result<CGPoint> {
        let value = self.element().position()?;
        let point = value.get_value()?;
        Ok(point)
    }

    fn set_position(&self, pos: CGPoint) -> Result<()> {
        let value = AXValue::from_CGPoint(pos)?;
        self.element()
            .set_attribute(&AXAttribute::position(), value)?;
        println!(
            "set_position desired:{:?} result:{:?}",
            pos,
            self.position()
        );
        Ok(())
    }

    fn size(&self) -> Result<CGSize> {
        let size = self.element().size()?.get_value()?;
        Ok(size)
    }

    fn set_size(&self, size: CGSize) -> Result<()> {
        let value = AXValue::from_CGSize(size)?;
        self.element().set_attribute(&AXAttribute::size(), value)?;
        println!("set_size desired:{:?} result:{:?}", size, self.size());
        Ok(())
    }

    fn frontmost_and_main(&self) -> Result<bool> {
        let app_is_frontmost = self.application()?.frontmost()?.into();
        let window_is_main = self.element().main()?.into();
        Ok(app_is_frontmost && window_is_main)
    }

    fn frame(&self) -> Result<CGRect> {
        let position = self.position()?;
        let size = self.size()?;
        Ok(CGRect::new(&position, &size))
    }

    fn set_frame(&self, frame: CGRect) -> Result<()> {
        let app = self.application()?;
        let enhanced_user_interface: AXAttribute<CFType> = AXAttribute::new(
            &CFString::from_static_string(kAXEnhancedUserInterfaceAttribute),
        );
        let is_enhanced_ui: bool = app
            .attribute(&enhanced_user_interface)?
            .downcast_into::<CFBoolean>()
            .unwrap()
            .into();
        if is_enhanced_ui {
            // This seems to always fail with error kAXErrorNotImplemented: -25208
            // But it still has the desired effect.
            let result = app.set_attribute(
                &enhanced_user_interface,
                CFBoolean::false_value().as_CFType(),
            );
            match result {
                Ok(())
                | Err(accessibility::Error::Ax(accessibility_sys::kAXErrorNotImplemented)) => (),
                Err(_) => result?,
            }
        }

        self.set_size(frame.size)?;
        self.set_position(frame.origin)?;
        self.set_size(frame.size)
    }

    /// Bring this window's application to front, and set this window as main.
    fn activate(&self) -> Result<()> {
        self.element().set_main(true)?;
        let pid = self.element().pid()?;
        unsafe {
            let app = NSRunningApplication::runningApplicationWithProcessIdentifier(NSApp(), pid);
            app.activateWithOptions_(
                NSApplicationActivationOptions::NSApplicationActivateIgnoringOtherApps,
            );
        };
        Ok(())
    }

    fn display(&self) -> Result<CGDisplay> {
        let position = self.position()?;
        let (displays, _) = CGDisplay::displays_with_point(position, 1).map_err(CGErrorWrapper)?;
        let display_id = displays.first().ok_or(accessibility::Error::NotFound)?;
        let display = CGDisplay::new(*display_id);
        Ok(display)
    }

    fn minimized(&self) -> Result<bool> {
        let b = self.element().minimized()?.into();
        Ok(b)
    }

    fn set_minimized(&self, minimized: bool) -> Result<()> {
        self.element()
            .set_attribute(&AXAttribute::minimized(), minimized)?;
        Ok(())
    }

    fn close(&self) -> Result<()> {
        let close_button_attr: AXAttribute<CFType> =
            AXAttribute::new(&CFString::from_static_string(kAXCloseButtonAttribute));
        let btn = self
            .element()
            .attribute(&close_button_attr)?
            .downcast_into::<AXUIElement>();
        if let Some(btn) = btn {
            btn.perform_action(&CFString::from_static_string(kAXPressAction))?;
            Ok(())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowWrapper<T> {
    id: uuid::Uuid,
    element: T,
}

impl<T> WindowWrapper<T> {
    pub fn new(element: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            element,
        }
    }

    pub fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

impl WindowWrapper<AXUIElement> {
    fn from_ui_element(element: AXUIElement) -> Result<Self> {
        let element_is_window = match element.role() {
            Ok(role) => role == CFString::from_static_string(accessibility_sys::kAXWindowRole),
            _ => false,
        };

        let window = if element_is_window {
            Ok(element)
        } else {
            element.window()
        }?;

        Ok(Self::new(window))
    }

    pub fn at_point(point: &CGPoint) -> Result<Option<Self>> {
        let result = AXUIElement::system_wide().element_at_position(point.x as f32, point.y as f32);
        let result = match result {
            Ok(el) => Ok(Some(el)),
            Err(accessibility::Error::Ax(accessibility_sys::kAXErrorNoValue)) => Ok(None),
            Err(e) => Err(e),
        };
        let element = result?;

        match element {
            None => Ok(None),
            Some(element) => {
                let w = Self::from_ui_element(element)?;
                Ok(Some(w))
            }
        }
    }

    fn _active() -> Result<Self> {
        let element = AXUIElement::system_wide().focused_uielement()?;
        Self::from_ui_element(element)
    }
}

impl Window for WindowWrapper<AXUIElement> {
    fn element(&self) -> &AXUIElement {
        &self.element
    }
}

impl Window for WindowWrapper<&AXUIElement> {
    fn element(&self) -> &AXUIElement {
        &self.element
    }
}

impl<'a> Window for WindowWrapper<ItemRef<'a, AXUIElement>> {
    fn element(&self) -> &AXUIElement {
        self.element.deref()
    }
}
