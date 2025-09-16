use std::mem::size_of;

fn main() {
    println!("NCCS = {}", nc::NCCS);
    println!("sizeof(termios) = {}", size_of::<nc::termios_t>());
    println!("sizeof(termios2) = {}", size_of::<nc::termios2_t>());
}
