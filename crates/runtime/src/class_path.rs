use std::{
    fs::File,
    io::{Cursor, Read, Seek},
    path::PathBuf,
};

use crate::jimage::Archive;

pub trait SeekRead: Read + Seek {}
impl<T: Seek + Read> SeekRead for T {}

pub enum ClassPath<'a> {
    Directory(PathBuf),
    JImage(Archive<'a>),
    Composite(Vec<ClassPath<'a>>),
}
impl ClassPath<'_> {
    pub fn find_resource(&self, name: &str) -> Option<Box<dyn SeekRead + '_>> {
        match self {
            Self::JImage(archive) => Some(Box::new(Cursor::new(
                archive.by_name(&format!("/java.base/{}", name))?.bytes(),
            ))),
            Self::Directory(path) => Some(Box::new(File::open(path.join(name)).ok()?)),
            Self::Composite(class_paths) => class_paths
                .iter()
                .flat_map(|cp| cp.find_resource(name).into_iter())
                .next(),
        }
    }
}
