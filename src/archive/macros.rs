#[macro_export]
macro_rules! assert_eq_some {
    ($left:expr, $right:expr) => {
        assert_eq!($left, Some($right))
    };
}

#[macro_export]
macro_rules! assert_none {
    ($left:expr) => {
        assert_eq!($left, None)
    };
}
