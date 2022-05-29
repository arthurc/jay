use std::{
    fs::File,
    io::{Cursor, Read, Seek},
    path::PathBuf,
};

use crate::jimage;

pub trait SeekRead: Read + Seek {}
impl<T: Seek + Read> SeekRead for T {}

pub trait ClassPath {
    fn find_resource(&self, name: &str) -> Option<Box<dyn SeekRead + '_>>;
}

impl ClassPath for jimage::Archive<'_> {
    fn find_resource(&self, name: &str) -> Option<Box<dyn SeekRead + '_>> {
        Some(Box::new(Cursor::new(
            self.by_name(&format!("/java.base/{}", name))?.bytes(),
        )))
    }
}

pub struct DirClassPath(PathBuf);
impl DirClassPath {
    pub fn new(path: PathBuf) -> Option<DirClassPath> {
        if !path.is_dir() {
            return None;
        }

        Some(Self(path))
    }
}
impl ClassPath for DirClassPath {
    fn find_resource(&self, name: &str) -> Option<Box<dyn SeekRead>> {
        Some(Box::new(File::open(self.0.join(name)).ok()?))
    }
}

impl ClassPath for Box<[&'_ dyn ClassPath]> {
    fn find_resource(&self, name: &str) -> Option<Box<dyn SeekRead + '_>> {
        self.iter()
            .flat_map(|cp| cp.find_resource(name).into_iter())
            .next()
    }
}
