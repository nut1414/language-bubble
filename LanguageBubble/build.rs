fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("resources/app.ico");
    res.compile().expect("Failed to compile Windows resources");
}
