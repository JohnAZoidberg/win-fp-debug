use colored::Colorize;

pub fn print_header(title: &str) {
    println!();
    println!("{}", format!("=== {} ===", title).bold().cyan());
}

pub fn print_pass(msg: &str) {
    println!("  {} {}", "[PASS]".bold().green(), msg);
}

pub fn print_fail(msg: &str) {
    println!("  {} {}", "[FAIL]".bold().red(), msg);
}

pub fn print_warn(msg: &str) {
    println!("  {} {}", "[WARN]".bold().yellow(), msg);
}

pub fn print_info(label: &str, value: &str) {
    println!("  {}: {}", label.bold(), value);
}

pub fn print_step(msg: &str) {
    println!("  {} {}", "-->".bold().blue(), msg);
}
