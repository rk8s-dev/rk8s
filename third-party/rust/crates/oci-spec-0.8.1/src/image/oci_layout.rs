use crate::{
    error::{OciSpecError, Result},
    from_file, from_reader, to_file, to_string, to_writer,
};
use derive_builder::Builder;
use getset::{Getters, Setters};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};

#[derive(Builder, Clone, Debug, Deserialize, Eq, Getters, Setters, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[builder(
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// The oci layout JSON object serves as a marker for the base of an Open Container Image Layout
/// and to provide the version of the image-layout in use. The imageLayoutVersion value will align
/// with the OCI Image Specification version at the time changes to the layout are made, and will
/// pin a given version until changes to the image layout are required.
pub struct OciLayout {
    /// This REQUIRED property specifies the image layout version.
    #[getset(get = "pub", set = "pub")]
    image_layout_version: String,
}

impl OciLayout {
    /// Attempts to load an oci layout from a file.
    /// # Errors
    /// This function will return an [OciSpecError::Io](crate::OciSpecError::Io)
    /// if the file does not exist or an
    /// [OciSpecError::SerDe](crate::OciSpecError::SerDe) if the oci layout
    /// cannot be deserialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<OciLayout> {
        from_file(path)
    }

    /// Attempts to load an oci layout from a stream.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe)
    /// if the oci layout cannot be deserialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    /// use std::fs::File;
    ///
    /// let reader = File::open("oci-layout").unwrap();
    /// let oci_layout = OciLayout::from_reader(reader).unwrap();
    /// ```
    pub fn from_reader<R: Read>(reader: R) -> Result<OciLayout> {
        from_reader(reader)
    }

    /// Attempts to write an oci layout to a file as JSON. If the file already exists, it
    /// will be overwritten.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// oci_layout.to_file("oci-layout").unwrap();
    /// ```
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        to_file(&self, path, false)
    }

    /// Attempts to write an oci layout to a file as pretty printed JSON. If the file
    /// already exists, it will be overwritten.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// oci_layout.to_file_pretty("my-oci-layout").unwrap();
    /// ```
    pub fn to_file_pretty<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        to_file(&self, path, true)
    }

    /// Attempts to write an oci layout to a stream as JSON.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// let mut writer = Vec::new();
    /// oci_layout.to_writer(&mut writer);
    /// ```
    pub fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        to_writer(&self, writer, false)
    }

    /// Attempts to write an oci layout to a stream as pretty printed JSON.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// let mut writer = Vec::new();
    /// oci_layout.to_writer_pretty(&mut writer);
    /// ```
    pub fn to_writer_pretty<W: Write>(&self, writer: &mut W) -> Result<()> {
        to_writer(&self, writer, true)
    }

    /// Attempts to write an oci layout to a string as JSON.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout configuration cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// let json_str = oci_layout.to_string().unwrap();
    /// ```
    pub fn to_string(&self) -> Result<String> {
        to_string(&self, false)
    }

    /// Attempts to write an oci layout to a string as pretty printed JSON.
    /// # Errors
    /// This function will return an [OciSpecError::SerDe](crate::OciSpecError::SerDe) if
    /// the oci layout configuration cannot be serialized.
    /// # Example
    /// ``` no_run
    /// use oci_spec::image::OciLayout;
    ///
    /// let oci_layout = OciLayout::from_file("oci-layout").unwrap();
    /// let json_str = oci_layout.to_string_pretty().unwrap();
    /// ```
    pub fn to_string_pretty(&self) -> Result<String> {
        to_string(&self, true)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn create_oci_layout() -> OciLayout {
        OciLayoutBuilder::default()
            .image_layout_version("lorem ipsum")
            .build()
            .expect("build oci layout")
    }

    fn get_oci_layout_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test/data/oci-layout")
    }

    #[test]
    fn load_oci_layout_from_file() {
        // arrange
        let oci_layout_path = get_oci_layout_path();

        // act
        let actual = OciLayout::from_file(oci_layout_path).expect("from file");

        // assert
        let expected = create_oci_layout();
        assert_eq!(actual, expected);
    }

    #[test]
    fn load_oci_layout_from_reader() {
        // arrange
        let reader = fs::read(get_oci_layout_path()).expect("read oci-layout");

        // act
        let actual = OciLayout::from_reader(&*reader).expect("from reader");

        // assert
        let expected = create_oci_layout();
        assert_eq!(actual, expected);
    }

    #[test]
    fn save_oci_layout_to_file() {
        // arrange
        let tmp = std::env::temp_dir().join("save_oci_layout_to_file");
        fs::create_dir_all(&tmp).expect("create test directory");
        let oci_layout = create_oci_layout();
        let oci_layout_path = tmp.join("oci-layout");

        // act
        oci_layout
            .to_file_pretty(&oci_layout_path)
            .expect("write oci-layout to file");

        // assert
        let actual = fs::read_to_string(oci_layout_path).expect("read actual");
        let expected = fs::read_to_string(get_oci_layout_path()).expect("read expected");
        assert_eq!(actual, expected);
    }

    #[test]
    fn save_oci_layout_to_writer() {
        // arrange
        let mut actual = Vec::new();
        let oci_layout = create_oci_layout();

        // act
        oci_layout.to_writer_pretty(&mut actual).expect("to writer");

        // assert
        let expected = fs::read(get_oci_layout_path()).expect("read expected");
        assert_eq!(actual, expected);
    }

    #[test]
    fn save_oci_layout_to_string() {
        // arrange
        let oci_layout = create_oci_layout();

        // act
        let actual = oci_layout.to_string_pretty().expect("to string");

        // assert
        let expected = fs::read_to_string(get_oci_layout_path()).expect("read expected");
        assert_eq!(actual, expected);
    }
}
