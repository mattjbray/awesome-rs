use accessibility::AXUIElement;
use anyhow::Result;
use core_graphics::display::{CGPoint, CGRect, CGSize};

use crate::{window::WindowWrapper, Window};

#[derive(Debug)]
pub enum Layout {
    Floating,
    Cascade,
}

impl Layout {
    pub fn apply<'a, T>(&self, windows: T) -> Result<()>
    where
        T: DoubleEndedIterator<Item = &'a WindowWrapper<AXUIElement>>,
    {
        match self {
            Layout::Floating => Ok(()),
            Layout::Cascade => self.apply_cascade(windows),
        }
    }

    fn apply_cascade<'a, T>(&self, windows: T) -> Result<()>
    where
        T: DoubleEndedIterator<Item = &'a WindowWrapper<AXUIElement>>,
    {
        for (i, w) in windows.rev().enumerate() {
            let d = w.display()?.bounds();
            let rect = CGRect::new(
                &CGPoint::new(
                    d.origin.x + i as f64 * 32.,
                    d.origin.y + 38. + i as f64 * 32.,
                ),
                &CGSize::new(d.size.width * 2. / 3., d.size.height * 2. / 3.),
            );
            w.set_frame(rect)
                .unwrap_or_else(|e| eprintln!("Could not set_frame on window {:?}: {:?}", w, e));
        }
        Ok(())
    }
}
