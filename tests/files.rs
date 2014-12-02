#[cfg(test)]
mod file {
    use std::os;
    use libbittorrent::files::File;

    fn name()     -> String { "こんにちは".into_string() }
    fn path_abs() -> Path   { os::tmpdir().join(name()) }
    fn path_rel() -> Path   { Path::new(name()) }
    static LEN: u64 = 256;

    #[test]
    #[should_fail]
    fn new_fail() {
        File::new(name(), path_rel(), LEN);
    }
    
    #[test]
    fn mv() {
        let mut f = File::new(name(), path_abs(), LEN);
        let p = os::tmpdir().join("あ");

        if let Err(e) = f.mv(p.clone()) {
            panic!("Could not move file. {}", e)
        }
        assert!(f.path() == &p);

        if let Ok(_) = f.mv(Path::new("あ")) {
            panic!("Moved file to relative path")
        }
    }

    #[test]
    fn set_path() {
        let mut f = File::new(name(), path_abs(), LEN);
        let p = os::tmpdir().join("あ");

        unsafe { f.set_path(p.clone()); }
        assert!(f.path() == &p);
    }

    #[test]
    #[should_fail]
    fn set_path_fail() {
        let mut f = File::new(name(), path_abs(), LEN);

        unsafe { f.set_path(Path::new("あ")); }
    }
}

#[cfg(test)]
mod directory {
    use std::os;
    use libbittorrent::files::Directory;

    fn name()     -> String { "こんにちは".into_string() }
    fn path_abs() -> Path   { os::tmpdir().join(name()) }

    #[test]
    #[should_fail]
    fn new_fail() {
        Directory::new(Path::new(""));
    }

    #[test]
    fn mv_rel_path() {
        let mut d = Directory::new(path_abs());

        if let Ok(_) = d.mv(Path::new("")) {
            panic!("Moved directory to relative path");
        }
    }
}
