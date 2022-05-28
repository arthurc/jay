use std::{fs::File, io::Read, path::PathBuf};

use crate::jimage;

pub trait ClassPath {
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>>;
}

impl ClassPath for jimage::Archive<'_> {
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>> {
        self.by_name(name).map(|r| r.bytes().into())
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
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>> {
        let mut file = File::open(self.0.join(name)).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;
        Some(buf.into_boxed_slice())
    }
}

impl<'a> ClassPath for Box<[&'a dyn ClassPath]> {
    fn find_resource(&self, name: &str) -> Option<Box<[u8]>> {
        self.iter()
            .flat_map(|cp| cp.find_resource(name).into_iter())
            .next()
    }
}
