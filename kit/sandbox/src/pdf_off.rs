//! PDF documents, in a build with no PDF reader (no `pdf` feature): a PDF a page shows is refused
//! as an image that could not be decoded, and the page is told so as for any file that is no image.

use std::sync::Arc;

use pencere_view::Size;

use crate::decode::{Failure, Part, Raster};

/// What a refused PDF is told as.
const WHY: &str = "this build has no PDF reader";

/// A parsed PDF: never made, `parse` refuses every file.
pub(crate) struct Document {
    sizes: Vec<Size>,
}

impl Document {
    /// Refuses the file: this build cannot read a PDF.
    pub(crate) fn parse(_bytes: &Arc<[u8]>) -> Result<Self, Failure> {
        Err(Failure::Broken(WHY.into()))
    }

    pub(crate) fn sizes(&self) -> &[Size] {
        &self.sizes
    }

    pub(crate) fn bytes(&self) -> usize {
        0
    }

    pub(crate) fn working(&self, _width: u32, _height: u32) -> usize {
        0
    }

    pub(crate) fn draw_part(
        &self,
        _index: usize,
        _width: u32,
        _height: u32,
        _part: Part,
    ) -> Result<Raster, Failure> {
        Err(Failure::Broken(WHY.into()))
    }
}
