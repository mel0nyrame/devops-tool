use std::{io, path::Path, ptr};

use objc2::rc::autoreleasepool;
use objc2_foundation::{
    NSData, NSDictionary, NSPropertyListMutabilityOptions, NSPropertyListSerialization, NSString,
};

use crate::locations::read_metadata_file;

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
    let bytes = read_metadata_file(path, "Info.plist")?;

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
