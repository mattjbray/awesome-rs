use accessibility::AXUIElement;
use anyhow::Result;
use core_graphics::display::{CGPoint, CGRect, CGSize};

use crate::{window::WindowWrapper, Window};

#[derive(Debug)]
pub struct TileHorizontalOpts {
    pub max_num_left: i32,
    pub primary_column_pct: u8,
}

#[derive(Debug)]
pub enum Layout {
    Floating,
    Cascade,
    TileHorizontal(TileHorizontalOpts),
}

type Windows = Vec<WindowWrapper<AXUIElement>>;

impl Layout {
    pub fn floating() -> Self {
        Self::Floating
    }
    pub fn cascade() -> Self {
        Self::Cascade
    }
    pub fn tile_horizontal(max_num_left: i32, primary_column_width_pct: u8) -> Self {
        Self::TileHorizontal(TileHorizontalOpts {
            max_num_left,
            primary_column_pct: primary_column_width_pct,
        })
    }

    pub fn apply(&self, windows: &Windows) -> Result<()> {
        match self {
            Layout::Floating => Ok(()),
            Layout::Cascade => self.apply_cascade(windows),
            Layout::TileHorizontal(opts) => self.apply_tile_horizontal(windows, &opts),
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

    fn apply_tile_horizontal(&self, windows: &Windows, opts: &TileHorizontalOpts) -> Result<()> {
        let num_windows = windows.len() as i32;

        if num_windows == 0 {
            return Ok(());
        };

        let d = windows[0].display()?.bounds();

        let num_left = i32::min(num_windows, opts.max_num_left);
        let num_right = if num_windows > num_left {
            num_windows - num_left
        } else {
            0
        };

        // Left column

        let left_width = if num_right == 0 {
            d.size.width
        } else {
            d.size.width * (opts.primary_column_pct as f64 / 100.)
        };

        let left_height = (d.size.height - 38.) / num_left as f64;
        let left_size = CGSize::new(left_width, left_height);

        for (i, w) in windows.iter().take(num_left as usize).enumerate() {
            let rect = CGRect::new(
                &CGPoint::new(d.origin.x, d.origin.y + 38. + i as f64 * left_height),
                &left_size,
            );
            w.set_frame(rect)
                .unwrap_or_else(|e| eprintln!("Could not set_frame on window {:?}: {:?}", w, e));
        }

        if num_right == 0 {
            return Ok(());
        };

        // Right column

        let right_width = d.size.width * ((100 - opts.primary_column_pct) as f64 / 100.);
        let right_height = (d.size.height - 38.) / num_right as f64;
        let right_size = CGSize::new(right_width, right_height);

        for (i, w) in windows.iter().skip(num_left as usize).enumerate() {
            let rect = CGRect::new(
                &CGPoint::new(
                    d.origin.x + left_width,
                    d.origin.y + 38. + i as f64 * right_height,
                ),
                &right_size,
            );
            w.set_frame(rect)
                .unwrap_or_else(|e| eprintln!("Could not set_frame on window {:?}: {:?}", w, e));
        }

        Ok(())
    }
}
