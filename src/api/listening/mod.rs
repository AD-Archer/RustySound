//! Listening data: the play counts and last-played dates servers keep, and
//! the one place they are aggregated.
//!
//! - [`aggregate`]: ranking by plays and tallies, shared by Stats' Most
//!   Played, Discover's Rewind and top artists, and Wrapped.
//! - [`wrapped`]: one year of listening (pure), and how it approximates a
//!   year from lifetime counts (see its module docs).
//! - [`load_wrapped`] / [`wrapped_years`]: read the history and work a year
//!   out, cached per year.

pub mod aggregate;
mod history;
pub mod wrapped;

pub use history::{current_year, load_wrapped, wrapped_years};
pub use wrapped::{DayPart, Personality, Ranked, WrappedYear};
