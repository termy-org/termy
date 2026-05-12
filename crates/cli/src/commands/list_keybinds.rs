pub fn run() {
    for line in super::providers::list_keybind_lines() {
        println!("{line}");
    }
}
