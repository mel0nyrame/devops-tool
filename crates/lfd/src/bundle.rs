use std::{
    fs::OpenOptions,
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    ptr,
};

use objc2::rc::autoreleasepool;
use objc2_foundation::{
    NSData, NSDictionary, NSPropertyListMutabilityOptions, NSPropertyListSerialization, NSString,
};

pub struct BundleMetadata {
    pub display_name: Option<String>,
    pub name: Option<String>,
    pub identifier: Option<String>,
}

pub struct ParsedBundle {
    pub metadata: BundleMetadata,
    pub invalid_keys: Vec<&'static str>,
}

pub fn read_metadata(path: &Path) -> io::Result<ParsedBundle> {
    const MAX_METADATA_SIZE: u64 = 4 * 1024 * 1024;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("Info.plist is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_METADATA_SIZE {
        return Err(io::Error::other("Info.plist exceeds 4 MiB"));
    }

    autoreleasepool(|_| {
        let data = NSData::with_bytes(&bytes);
        // SAFETY: The optional output format pointer is null.
        let plist = unsafe {
            NSPropertyListSerialization::propertyListWithData_options_format_error(
                &data,
                NSPropertyListMutabilityOptions::Immutable,
                ptr::null_mut(),
            )
        }
        .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        let dictionary = plist
            .downcast::<NSDictionary>()
            .map_err(|_| io::Error::other("Info.plist is not a dictionary"))?;
        let mut invalid_keys = Vec::new();
        let mut read = |key: &'static str| -> Option<String> {
            let value = dictionary.objectForKey(&NSString::from_str(key))?;
            match value.downcast::<NSString>() {
                Ok(value) => {
                    let value = value.to_string();
                    (!value.trim().is_empty()).then_some(value)
                }
                Err(_) => {
                    invalid_keys.push(key);
                    None
                }
            }
        };
        let metadata = BundleMetadata {
            display_name: read("CFBundleDisplayName"),
            name: read("CFBundleName"),
            identifier: read("CFBundleIdentifier"),
        };
        Ok(ParsedBundle {
            metadata,
            invalid_keys,
        })
    })
}
