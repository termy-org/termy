pub fn run() {
    for theme in super::providers::list_theme_lines() {
        println!("{theme}");
    }
}
