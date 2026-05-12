pub fn run() {
    for line in super::providers::list_color_lines() {
        println!("{line}");
    }
}
