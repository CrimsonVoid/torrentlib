macro_rules! unwrap {
    ($sp:path, $inp:expr) => {
        unwrap!($sp, $inp, None)
    };

    ($sp:path, $inp:expr, $ret:ident) => {
        match $inp {
            $sp(v) => v,
            _      => return $ret,
        }
    }
}

macro_rules! unwrap_opt {
    ($sp:path, $inp:expr) => {
        unwrap_opt!($sp, $inp, None)
    };

    ($sp:path, $inp:expr, $ret:ident) => {
        match $inp {
            Some($sp(v)) => v,
            _            => return $ret,
        }
    }
}
