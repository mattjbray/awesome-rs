use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes, AXValue};
use accessibility_sys::kAXApplicationRole;
use core_foundation::{
    base::{CFType, TCFType},
    boolean::CFBoolean,
    string::CFString,
};
use core_graphics::display::{CGDisplay, CGPoint, CGRect, CGSize};

#[allow(non_upper_case_globals)]
const kAXEnhancedUserInterfaceAttribute: &str = "AXEnhancedUserInterface";

pub trait Window {
    fn element(&self) -> &AXUIElement;

    fn application(&self) -> Result<AXUIElement, accessibility::Error> {
        let element = self.element();
        let role = element.role()?;
        if role == CFString::from_static_string(kAXApplicationRole) {
            Ok(element.clone())
        } else {
            let pid = element.pid()?;
            Ok(AXUIElement::application(pid))
        }
    }

    fn position(&self) -> Result<CGPoint, accessibility::Error> {
        let value = self.element().position()?;
        value.get_value()
    }

    fn set_position(&self, pos: CGPoint) -> Result<(), accessibility::Error> {
        self.element().set_attribute(
            &AXAttribute::position(),
            AXValue::from_CGPoint(pos).unwrap(),
        )?;
        println!(
            "set_position desired:{:?} result:{:?}",
            pos,
            self.position()
        );
        Ok(())
    }

    fn size(&self) -> Result<CGSize, accessibility::Error> {
        let value = self.element().size()?;
        value.get_value()
    }

    fn set_size(&self, size: CGSize) -> Result<(), accessibility::Error> {
        self.element()
            .set_attribute(&AXAttribute::size(), AXValue::from_CGSize(size).unwrap())?;
        println!("set_size desired:{:?} result:{:?}", size, self.size());
        Ok(())
    }

    fn frame(&self) -> Result<CGRect, accessibility::Error> {
        let position = self.position()?;
        let size = self.size()?;
        Ok(CGRect::new(&position, &size))
    }

    fn set_frame(&self, frame: CGRect) -> Result<(), accessibility::Error> {
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
                Err(_) => return result,
            }
        }

        self.set_size(frame.size)?;
        self.set_position(frame.origin)?;
        self.set_size(frame.size)
    }

    /// Bring this window's application to front, and set this window as main.
    fn activate(&self) -> Result<(), accessibility::Error> {
        let app = self.application()?;
        app.set_attribute(&AXAttribute::frontmost(), true)?;
        self.element().set_main(true)
    }

    fn display(&self) -> Result<CGDisplay, accessibility::Error> {
        let position = self.position()?;
        let (displays, _) = CGDisplay::displays_with_point(position, 1).unwrap();
        let display_id = displays.first().ok_or(accessibility::Error::NotFound)?;
        let display = CGDisplay::new(*display_id);
        Ok(display)
    }
}
