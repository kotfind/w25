// -------------------- Log --------------------

macro_rules! trace {
    ($($ts:tt)*) => {
        #[cfg(feature = "log")]
        ::log::trace!($($ts)*);
    };
}
pub(crate) use trace;
