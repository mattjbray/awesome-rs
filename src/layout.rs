use accessibility::AXUIElement;
use anyhow::Result;
use core_graphics::display::{CGPoint, CGRect, CGSize};

use crate::{window::WindowWrapper, Window};

#[derive(Debug)]
pub enum Layout {
    Floating,
    Cascade,
    TileHorizontal,
}

type Windows = Vec<WindowWrapper<AXUIElement>>;

impl Layout {
    pub fn apply(&self, windows: &Windows) -> Result<()> {
        match self {
            Layout::Floating => Ok(()),
            Layout::Cascade => self.apply_cascade(windows),
            Layout::TileHorizontal => self.apply_tile_horizontal(windows),
        }
    }

    fn apply_cascade(&self, windows: &Windows) -> Result<()> {
        for (i, w) in windows.iter().rev().enumerate() {
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

    fn apply_tile_horizontal(&self, windows: &Windows) -> Result<()> {
        let num_windows = windows.len() as i32;

        if num_windows == 0 {
            return Ok(());
        };

        let d = windows[0].display()?.bounds();

        let num_left = i32::min(num_windows, 2);
        let num_right = if num_windows > num_left {
            num_windows - num_left
        } else {
            0
        };

        // Left column

        let width = if num_right == 0 {
            d.size.width
        } else {
            d.size.width / 2.
        };

        let height = (d.size.height - 38.) / num_left as f64;

        for (i, w) in windows.iter().take(num_left as usize).enumerate() {
            let rect = CGRect::new(
                &CGPoint::new(d.origin.x, d.origin.y + 38. + i as f64 * height),
                &CGSize::new(width, height),
            );
            w.set_frame(rect)
                .unwrap_or_else(|e| eprintln!("Could not set_frame on window {:?}: {:?}", w, e));
        }

        if num_right == 0 {
            return Ok(());
        };

        // Right column

        let width = d.size.width / 2. ;

        let height = (d.size.height - 38.) / num_right as f64;

        for (i, w) in windows.iter().skip(num_left as usize).enumerate() {
            let rect = CGRect::new(
                &CGPoint::new(d.origin.x + width, d.origin.y + 38. + i as f64 * height),
                &CGSize::new(width, height),
            );
            w.set_frame(rect)
                .unwrap_or_else(|e| eprintln!("Could not set_frame on window {:?}: {:?}", w, e));
        }

        Ok(())
    }
}
