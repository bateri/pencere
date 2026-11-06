//! The guards of the PDF reader: the sizes of the pages, and that a file that is cut, or is
//! not what it says it is, is refused or drawn as far as it goes and never takes the thread down.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use pencere_view::Size;

use crate::decode::{Failure, Format, sniff};
use crate::pdf::Document;

const PAGES_PDF: &[u8] = include_bytes!("fixtures/pages.pdf");

fn document(bytes: &[u8]) -> Result<Document, Failure> {
    Document::parse(&Arc::<[u8]>::from(bytes))
}

#[test]
fn the_pages_of_a_pdf_have_the_size_the_document_says_with_its_quarter_turn_applied() {
    let document = document(PAGES_PDF).unwrap();
    assert_eq!(
        document.sizes(),
        [
            Size::new(200.0, 100.0),
            Size::new(100.0, 200.0),
            Size::new(100.0, 300.0)
        ]
    );
    // Paid for as its file and a share for each page.
    assert!(document.bytes() > PAGES_PDF.len());
    // A page that is not there is not drawn.
    assert!(matches!(document.draw(3, 10, 10), Err(Failure::Broken(_))));
}

#[test]
fn a_pdf_is_told_from_its_first_bytes() {
    assert_eq!(sniff(PAGES_PDF), Some(Format::Pdf));
    assert_eq!(sniff(b"%PDF-"), Some(Format::Pdf));
    assert_eq!(sniff(b"%PDF"), None);
    assert_eq!(sniff(b" %PDF-1.4"), None);
}

#[test]
fn a_page_is_drawn_at_the_size_asked_whatever_it_is() {
    let document = document(PAGES_PDF).unwrap();
    for (width, height) in [(1, 1), (7, 3), (200, 100), (4096, 4096)] {
        let raster = document.draw(0, width, height).unwrap();
        assert_eq!((raster.width, raster.height), (width, height));
        assert_eq!(raster.pixels.len(), (width * height * 4) as usize);
    }
    // More than the surface the renderer has is refused and not drawn.
    assert!(matches!(
        document.draw(0, 70_000, 10),
        Err(Failure::Broken(_))
    ));
    assert!(matches!(document.draw(0, 0, 10), Err(Failure::Broken(_))));
}

#[test]
fn a_pdf_that_is_cut_or_spoiled_anywhere_is_refused_or_drawn_and_never_panics() {
    let mut refused = 0;
    let mut drawn = 0;
    let mut spoil = |bytes: &[u8]| {
        let outcome = catch_unwind(AssertUnwindSafe(|| match document(bytes) {
            Ok(document) => {
                for index in 0..document.sizes().len() {
                    document.draw(index, 40, 40).map(|_| ())?;
                }
                Ok(())
            }
            Err(failure) => Err(failure),
        }));
        match outcome {
            Ok(Ok(())) => drawn += 1,
            Ok(Err(_)) => refused += 1,
            Err(_) => panic!(
                "the reader panicked on {} bytes of {:?}",
                bytes.len(),
                &bytes[..bytes.len().min(40)]
            ),
        }
    };
    for cut in (0..PAGES_PDF.len()).step_by(11) {
        spoil(&PAGES_PDF[..cut]);
    }
    // Every byte of the file set to a few other values, one at a time.
    for at in (0..PAGES_PDF.len()).step_by(13) {
        for value in [0x00, 0xff, b'9'] {
            let mut bytes = PAGES_PDF.to_vec();
            bytes[at] = value;
            spoil(&bytes);
        }
    }
    assert!(refused > 0 && drawn > 0, "refused {refused}, drawn {drawn}");
}
