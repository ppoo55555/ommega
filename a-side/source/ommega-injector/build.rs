use std::{fs, path::PathBuf, process::Command};

fn add_all_aidl_sources(builder: rsbinder_aidl::Builder, dir: &str) -> rsbinder_aidl::Builder {
    let mut builder = builder;
    let entries = fs::read_dir(dir).unwrap();
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) == Some("aidl") {
            builder = builder.source(path);
        }
    }
    builder
}

fn main() {
    println!("cargo:rerun-if-changed=../aidl");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rustc-link-arg-bin=ommega-inject=-Wl,--export-dynamic-symbol=entry");

    let build_target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".to_string());
    if build_target == "i686-linux-android" {
        ensure_i686_clang_builtins_compat();
    }
    let package_version =
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown-version".to_string());
    let git_sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown-git".to_string());
    let build_id = format!("{package_version}-{git_sha}-{build_target}");

    println!("cargo:rustc-env=INJECTOR_BUILD_TARGET={build_target}");
    println!("cargo:rustc-env=INJECTOR_BUILD_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=INJECTOR_BUILD_ID={build_id}");

    let aidl_dirs = [
        "../aidl/android/system/keystore2",
        "../aidl/android/hardware/security/keymint",
        "../aidl/android/hardware/security/secureclock",
        "../aidl/android/security/authorization",
        "../aidl/android/security/maintenance",
        "../aidl/android/security/keystore",
        "../aidl/top/jiyin004/ommega",
        "../aidl/vendor/qti/hardware/soter",
    ];
    let mut aidl = rsbinder_aidl::Builder::new();
    for dir in aidl_dirs {
        aidl = aidl.include_dir(PathBuf::from(dir));
    }
    let mut aidl = aidl.output(PathBuf::from("aidl.rs"));
    for dir in aidl_dirs {
        aidl = add_all_aidl_sources(aidl, dir);
    }

    aidl.generate().unwrap();

    let generated_path = PathBuf::from(format!("{}/aidl.rs", std::env::var("OUT_DIR").unwrap()));
    let content = fs::read_to_string(&generated_path).unwrap();
    // rsbinder-aidl only puts `#[allow(clippy::all)]` on the first package tree it
    // emits, so the later ones (`top`, `vendor`) trip `clippy -- -D warnings` on
    // generated code we do not own (`identity_op` on `FIRST_CALL_TRANSACTION + 0`
    // for the first method of an interface, for one).  Give each of them the same
    // allow; this only ever rewrites the freshly generated file.
    let mut patched_content = content;
    for module in ["android", "top", "vendor"] {
        patched_content = patched_content.replace(
            &format!("\npub mod {module} {{"),
            &format!("\n#[allow(clippy::all)]\n#[allow(unused_imports)]\npub mod {module} {{"),
        );
    }
    let patched_content = patched_content.replace(
        "fn build_parcel_getNumberOfEntries(&self, _arg_domain: super::Domain::Domain, _arg_nspace: i64) -> rsbinder::Result<rsbinder::Parcel>",
        "pub(crate) fn build_parcel_getNumberOfEntries(&self, _arg_domain: super::Domain::Domain, _arg_nspace: i64) -> rsbinder::Result<rsbinder::Parcel>",
    );

    fs::write(&generated_path, &patched_content).unwrap();
}

fn ensure_i686_clang_builtins_compat() {
    let out_dir = match std::env::var("OUT_DIR") {
        Ok(val) => PathBuf::from(val),
        Err(_) => return,
    };
    let target_lib = out_dir.join("libclang_rt.builtins-x86-android.a");

    println!("cargo:rustc-link-search=native={}", out_dir.display());

    if target_lib.exists() {
        return;
    }

    let mut candidates = Vec::new();
    for env_var in [
        "ANDROID_NDK_ROOT",
        "ANDROID_NDK_HOME",
        "ANDROID_NDK",
        "NDK_HOME",
    ] {
        if let Ok(val) = std::env::var(env_var) {
            candidates.push(PathBuf::from(val));
        }
    }
    for env_var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Ok(val) = std::env::var(env_var) {
            candidates.push(PathBuf::from(&val).join("ndk"));
            candidates.push(PathBuf::from(&val).join("ndk-bundle"));
        }
    }

    fn find_builtins(dir: &std::path::Path) -> Option<PathBuf> {
        if !dir.is_dir() {
            return None;
        }
        let target_name = "libclang_rt.builtins-i686-android.a";
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            if let Ok(entries) = fs::read_dir(&current) {
                for entry in entries.filter_map(Result::ok) {
                    let path = entry.path();
                    if path.is_dir() {
                        if path
                            .components()
                            .count()
                            .saturating_sub(dir.components().count())
                            <= 8
                        {
                            stack.push(path);
                        }
                    } else if path.file_name().and_then(|s| s.to_str()) == Some(target_name) {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    let mut found = None;
    for cand in candidates {
        if let Some(path) = find_builtins(&cand) {
            found = Some(path);
            break;
        }
    }

    if let Some(src) = found {
        let _ = fs::copy(&src, &target_lib);
    } else {
        let _ = fs::write(&target_lib, b"!<arch>\n");
    }
}
