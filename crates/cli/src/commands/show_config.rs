pub fn run() {
    match super::providers::show_config_lines() {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
        }
        Err(error) => eprintln!("{error}"),
    }
}
