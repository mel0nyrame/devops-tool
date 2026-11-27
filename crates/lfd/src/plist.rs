use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    ptr,
};

use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{
    NSData, NSDictionary, NSPropertyListMutabilityOptions, NSPropertyListSerialization, NSString,
};

pub fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    const MAX_PLIST_SIZE: u64 = 4 * 1024 * 1024;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("property list is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PLIST_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PLIST_SIZE {
        return Err(io::Error::other("property list exceeds 4 MiB"));
    }

    Ok(bytes)
}

pub fn read_dictionary(path: &Path) -> io::Result<Retained<NSDictionary>> {
    parse_dictionary(&read_bytes(path)?)
}

pub fn parse_dictionary(bytes: &[u8]) -> io::Result<Retained<NSDictionary>> {
    autoreleasepool(|_| {
        let data = NSData::with_bytes(bytes);
        // SAFETY: The optional output format pointer is null.
        let plist = unsafe {
            NSPropertyListSerialization::propertyListWithData_options_format_error(
                &data,
                NSPropertyListMutabilityOptions::Immutable,
                ptr::null_mut(),
            )
        }
        .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        plist
            .downcast::<NSDictionary>()
            .map_err(|_| io::Error::other("property list is not a dictionary"))
    })
}

pub fn string(dictionary: &NSDictionary, key: &str) -> Option<String> {
    let value = dictionary.objectForKey(&NSString::from_str(key))?;
    let value = value.downcast::<NSString>().ok()?.to_string();
    (!value.trim().is_empty()).then_some(value)
}
