fn main() {
    println!("cargo:rerun-if-changed=Resources/app.ico");
    let mut res = winres::WindowsResource::new();
    res.set_icon("Resources/app.ico");
    res.compile().expect("Failed to compile Windows resources");
}
