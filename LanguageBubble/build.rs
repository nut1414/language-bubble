use std::env;

fn main() {
    println!("cargo:rerun-if-changed=Resources/app.ico");
    println!("cargo:rerun-if-env-changed=LANGUAGE_BUBBLE_BUILD_COMMIT");

    // CI sets LANGUAGE_BUBBLE_BUILD_COMMIT for nightly/branch builds so the shown
    // version identifies the commit. CARGO_PKG_VERSION stays untouched for update checks.
    let version = env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION is set by Cargo");
    let display_version = match env::var("LANGUAGE_BUBBLE_BUILD_COMMIT") {
        Ok(commit) if !commit.is_empty() => {
            let short: String = commit.chars().take(7).collect();
            format!("{version}+{short}")
        }
        _ => version,
    };
    println!("cargo:rustc-env=APP_DISPLAY_VERSION={display_version}");

    let mut res = winres::WindowsResource::new();
    res.set_icon("Resources/app.ico");
    res.set("ProductVersion", &display_version);
    res.compile().expect("Failed to compile Windows resources");
}
