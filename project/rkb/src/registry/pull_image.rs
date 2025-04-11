use dockerfile_parser::{FromFlag, ImageRef};

use anyhow::Result;

/// Pulls an image from a registry(remote or local).
/// 
/// returns the path to the image.
pub fn pull_image(_from_flags: &Vec<FromFlag>, _image_ref: &ImageRef) -> Result<String> {
    Ok(String::from("~/layers/diff"))
}