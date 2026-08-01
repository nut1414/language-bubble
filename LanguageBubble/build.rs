fn main() {
    println!("cargo:rerun-if-changed=Resources/app.rc");
    println!("cargo:rerun-if-changed=Resources/app.ico");
    let mut res = winres::WindowsResource::new();
    res.set_resource_file("Resources/app.rc");
    res.compile().expect("Failed to compile Windows resources");
}
