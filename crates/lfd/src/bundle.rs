use std::{io, path::Path};

use objc2_foundation::NSString;

use crate::plist::{parse_dictionary, read_bytes};

pub struct BundleMetadata {
    pub display_name: Option<String>,
    pub name: Option<String>,
    pub identifier: Option<String>,
}

pub struct ParsedBundle {
    pub metadata: BundleMetadata,
    pub invalid_keys: Vec<&'static str>,
}

pub fn has_app_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

pub fn read_metadata(path: &Path) -> io::Result<ParsedBundle> {
    parse_metadata(&read_bytes(path)?)
}

pub fn parse_metadata(bytes: &[u8]) -> io::Result<ParsedBundle> {
    let dictionary = parse_dictionary(bytes)?;
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
}
