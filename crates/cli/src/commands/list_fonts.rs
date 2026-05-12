pub fn run() {
    for font in super::providers::list_fonts_lines() {
        println!("{font}");
    }
}
