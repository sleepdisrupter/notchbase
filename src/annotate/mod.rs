//! Annotate: a window for drawing and writing on a screenshot. The picture is copied to the clipboard as it changes.

mod brush;
mod clock;
mod composite;
mod copier;
mod editor;
mod fit;
mod focus;
mod lettering;
mod mark;
mod raster;
mod rect;
mod sample;
mod sheet;
mod stroke;
#[cfg(test)]
mod testing;
mod window;

pub use brush::Brush;
pub use copier::{Copier, Job};
pub use editor::{CHROME, Editor, Outcome};
pub use fit::{Fit, SMALLEST_WINDOW, window_size};
pub use focus::Focus;
pub use lettering::{Lettering, Proof, Typesetter, Words};
pub use rect::PixelRect;
pub use sample::sample;
pub use sheet::Sheet;
pub use stroke::Stroke;
pub use window::Annotator;
