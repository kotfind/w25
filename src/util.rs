// -------------------- Log --------------------

macro_rules! debug {
    ($($ts:tt)*) => {
        #[cfg(feature = "log")]
        ::log::debug!($($ts)*);
    };
}
pub(crate) use debug;

macro_rules! trace {
    ($($ts:tt)*) => {
        #[cfg(feature = "log")]
        ::log::trace!($($ts)*);
    };
}
pub(crate) use trace;
