use std::{io, path::Path, ptr};

use objc2::rc::{Retained, autoreleasepool};
use objc2_foundation::{
    NSData, NSDictionary, NSPropertyListMutabilityOptions, NSPropertyListSerialization, NSString,
};

use crate::locations::read_metadata_file;

pub fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    read_metadata_file(path, "property list").map(|(bytes, _)| bytes)
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
