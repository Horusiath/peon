mod flatten;
mod merge;

pub use flatten::Flatten;
pub use merge::Merge;

pub type Value = smallvec::SmallVec<u8, 10>;

pub(crate) const TAG_BOOL_FALSE: u8 = 0b1000_0000;
pub(crate) const TAG_BOOL_TRUE: u8 = 0b1000_0001;
pub(crate) const TAG_STRING: u8 = 0b1000_0010;
pub(crate) const TAG_FLOAT: u8 = 0b1000_0011;
pub(crate) const TAG_NULL: u8 = 0b1000_0100;
pub(crate) const TAG_INTEGER: u8 = 0b0000_0000;
