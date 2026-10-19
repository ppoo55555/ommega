use std::{
    string::{String, ToString},
    sync::OnceLock,
};

static ANDROID_MAJOR_VERSION: OnceLock<Option<i32>> = OnceLock::new();
static HOST_ANDROID_MAJOR_VERSION: OnceLock<Option<i32>> = OnceLock::new();

pub fn android_major_version() -> Option<i32> {
    *ANDROID_MAJOR_VERSION.get_or_init(|| android_major_version_with(read_string_property))
}

pub fn android_major_version_with(read_property: impl Fn(&str) -> Option<String>) -> Option<i32> {
    read_property("ro.build.version.release_or_codename")
        .or_else(|| read_property("ro.build.version.release"))
        .and_then(|value| value.parse::<i32>().ok())
        .or_else(|| {
            read_property("ro.build.version.sdk")
                .and_then(|sdk| sdk.parse::<i32>().ok())
                .map(|sdk| match sdk {
                    31 | 32 => 12,
                    33 => 13,
                    34 => 14,
                    35 => 15,
                    36 => 16,
                    value if value >= 37 => 17,
                    _ => panic!("unrecognized or unsupported SDK version: {}", sdk),
                })
        })
}

pub fn host_android_major_version() -> Option<i32> {
    *HOST_ANDROID_MAJOR_VERSION
        .get_or_init(|| host_android_major_version_with(read_string_property))
}

pub fn host_android_major_version_with(
    read_property: impl Fn(&str) -> Option<String>,
) -> Option<i32> {
    // 优先使用 ro.build.version.sdk（API Level）反推真实宿主底包版本。
    // 在系统属性被模块伪装（例如把 release 伪装成 16）时，底层的 SDK API level
    // （例如安卓 13 的 33）代表着宿主系统的真实 Binder/C++ ABI 和系统服务形态。
    parse_sdk_property(&read_property, "ro.build.version.sdk")
        .or_else(|| parse_sdk_property(&read_property, "ro.system.build.version.sdk"))
        .or_else(|| parse_sdk_property(&read_property, "ro.vendor.build.version.sdk"))
        .or_else(|| android_major_version_with(&read_property))
}

fn parse_sdk_property(
    read_property: &impl Fn(&str) -> Option<String>,
    prop_name: &str,
) -> Option<i32> {
    read_property(prop_name)
        .and_then(|sdk| sdk.parse::<i32>().ok())
        .and_then(|sdk| match sdk {
            31 | 32 => Some(12),
            33 => Some(13),
            34 => Some(14),
            35 => Some(15),
            36 => Some(16),
            value if value >= 37 => Some(17),
            _ => None,
        })
}

fn read_string_property(name: &str) -> Option<String> {
    rsproperties::get::<String>(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_version_derivation_with_spoofed_release() {
        let props = |name: &str| match name {
            "ro.build.version.release" => Some("16".to_string()),
            "ro.build.version.sdk" => Some("33".to_string()),
            _ => None,
        };
        assert_eq!(android_major_version_with(props), Some(16));
        assert_eq!(host_android_major_version_with(props), Some(13));
    }

    #[test]
    fn test_host_version_derivation_without_spoofing() {
        let props = |name: &str| match name {
            "ro.build.version.release" => Some("13".to_string()),
            "ro.build.version.sdk" => Some("33".to_string()),
            _ => None,
        };
        assert_eq!(android_major_version_with(props), Some(13));
        assert_eq!(host_android_major_version_with(props), Some(13));
    }

    #[test]
    fn test_host_version_fallback_to_release_when_sdk_missing() {
        let props = |name: &str| match name {
            "ro.build.version.release" => Some("14".to_string()),
            _ => None,
        };
        assert_eq!(android_major_version_with(props), Some(14));
        assert_eq!(host_android_major_version_with(props), Some(14));
    }
}
