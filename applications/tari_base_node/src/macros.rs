macro_rules! try_or_print {
    ($e:expr, $($arg:tt)*) => {
        match $e {
            Ok(v) => v,
            Err(err) => {
                println!($($arg)*, error=err);
                return;
            },
        }
    };
    ($e:expr) => {
        try_or_print!($e, "Error: {error}")
    };
}
