//! PIC rendering support.
//!
//! # Example
//!
//! ```
//! use qrcode::QrCode;
//! use qrcode::render::pic;
//!
//! let code = QrCode::new(b"Hello").unwrap();
//! let pic = code.render::<pic::Color>().build();
//! println!("{}", pic);

#![cfg(feature = "pic")]

use std::fmt::Write;

use crate::render::{Canvas as RenderCanvas, Pixel};
use crate::types::Color as ModuleColor;

/// A PIC color.
#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Color;

impl Pixel for Color {
    type Canvas = Canvas;
    type Image = String;

    fn default_color(_color: ModuleColor) -> Self {
        Color
    }
}

#[doc(hidden)]
pub struct Canvas {
    pic: String,
}

impl RenderCanvas for Canvas {
    type Pixel = Color;
    type Image = String;

    fn new(width: u32, height: u32, _dark_pixel: Color, _light_pixel: Color) -> Self {
        Canvas {
            pic: format!(
                concat!(
                    "maxpswid={w};maxpsht={h};movewid=0;moveht=1;boxwid=1;boxht=1\n",
                    "define p {{ box wid $3 ht $4 fill 1 with .nw at $1,-$2 }}\n",
                    "box wid maxpswid ht maxpsht with .nw at 0,0\n",
                ),
                w = width,
                h = height
            ),
        }
    }

    fn draw_dark_pixel(&mut self, x: u32, y: u32) {
        self.draw_dark_rect(x, y, 1, 1);
    }

    fn draw_dark_rect(&mut self, left: u32, top: u32, width: u32, height: u32) {
        writeln!(self.pic, "p({left},{top},{width},{height})").unwrap();
    }

    fn into_image(self) -> String {
        self.pic
    }
}
