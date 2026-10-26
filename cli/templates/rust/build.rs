// Carries the page (src/view/) and the pictures (src/assets/) into the program: `pencere::app!()`
// is them. Under `pencere dev` the program reads them from the project instead, and shows the view
// again when one of them changes.
fn main() {
    pencere::build::app().expect("the files of src/view/ and src/assets/");
}
