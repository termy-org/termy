pub fn run() {
    for line in super::providers::list_action_lines() {
        println!("{}", line);
    }
}
